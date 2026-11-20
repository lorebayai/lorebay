use sea_orm::sea_query::{
    Alias, Expr, ExprTrait, Func, IntoColumnRef, JoinType, LikeExpr, OnConflict, Order, Query,
    SelectStatement,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DbBackend, DbErr, DeriveIden,
    EntityTrait, FromQueryResult, QueryFilter, QueryOrder, QuerySelect,
};
use typed_builder::TypedBuilder;

use crate::KnowledgeTarget;
use crate::entity::space_knowledge::{ActiveModel, Column, Entity, Model};
use crate::entity::space_knowledge_version::{Column as VersionColumn, Entity as VersionEntity};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct KnowledgeRepository<'a, C> {
    pub connection: &'a C,
}

#[derive(Clone, Debug, FromQueryResult, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct KnowledgeListRow {
    pub number: i64,
    pub name: String,
    pub summary: String,
    pub position: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct KnowledgeListRequest<'a> {
    pub space: &'a str,
    pub prefix: Option<&'a str>,
    pub keyword: Option<&'a str>,
    pub after: Option<&'a str>,
    pub limit: u64,
}

#[derive(Clone, Debug, FromQueryResult, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct DeletedKnowledgeRow {
    pub number: i64,
    pub name: String,
    pub applied_by: String,
    pub deleted_at: i64,
}

impl<C: ConnectionTrait> KnowledgeRepository<'_, C> {
    pub async fn by_name(&self, space: &str, name_key: &str) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::SpaceId.eq(space))
            .filter(Column::NameKey.eq(name_key))
            .one(self.connection)
            .await
    }

    pub async fn target(
        &self,
        space: &str,
        target: &KnowledgeTarget,
        lock: bool,
    ) -> Result<Option<Model>, DbErr> {
        let mut query = Entity::find().filter(Column::SpaceId.eq(space));
        query = if let Some(name) = &target.name {
            query.filter(Column::NameKey.eq(name.to_lowercase()))
        } else if let Some(number) = target.number() {
            query.filter(Column::Number.eq(number))
        } else {
            return Ok(None);
        };
        if lock && self.connection.get_database_backend() == DbBackend::Postgres {
            query = query.lock_exclusive();
        }
        query.one(self.connection).await
    }

    pub async fn insert(&self, model: ActiveModel) -> Result<u64, DbErr> {
        Entity::insert(model)
            .on_conflict(
                OnConflict::columns([Column::SpaceId, Column::NameKey])
                    .do_nothing()
                    .to_owned(),
            )
            .exec_without_returning(self.connection)
            .await
    }

    pub async fn update(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.update(self.connection).await
    }

    pub async fn list_deleted(
        &self,
        space: &str,
        after: Option<i64>,
        limit: u64,
    ) -> Result<Vec<DeletedKnowledgeRow>, DbErr> {
        let mut query = Entity::find()
            .select_only()
            .columns([Column::Number, Column::Name, Column::DeletedAt])
            .column(VersionColumn::AppliedBy)
            .join(
                JoinType::InnerJoin,
                Entity::belongs_to(VersionEntity)
                    .from(Column::Id)
                    .to(VersionColumn::KnowledgeId)
                    .on_condition(|_, _| {
                        Condition::all().add(
                            Expr::col((Entity, Column::LatestVersion))
                                .eq(Expr::col((VersionEntity, VersionColumn::Version))),
                        )
                    })
                    .into(),
            )
            .filter(Column::SpaceId.eq(space))
            .filter(Column::DeletedAt.is_not_null());
        if let Some(after) = after {
            query = query.filter(Column::Number.gt(after));
        }
        query
            .order_by_asc(Column::Number)
            .limit(limit)
            .into_model()
            .all(self.connection)
            .await
    }

    pub async fn list(
        &self,
        request: KnowledgeListRequest<'_>,
    ) -> Result<Vec<KnowledgeListRow>, DbErr> {
        let backend = self.connection.get_database_backend();
        let mut query = latest_rows(request.space);
        let position = if let Some(keyword) = request.keyword {
            filter_page(&mut query, backend, &request);
            query.cond_where(keyword_match(backend, keyword));
            Expr::col((Entity, Column::NameKey))
        } else {
            query.join_subquery(
                JoinType::InnerJoin,
                grouped_page(backend, &request),
                KnowledgePage::Table,
                Expr::col((Entity, Column::Number))
                    .eq(Expr::col((KnowledgePage::Table, KnowledgePage::Number))),
            );
            Expr::col((KnowledgePage::Table, KnowledgePage::Position))
        };
        query
            .expr_as(position.clone(), KnowledgePage::Position)
            .order_by_expr(collated(backend, position), Order::Asc)
            .limit(request.limit);
        self.connection
            .query_all(&query)
            .await?
            .iter()
            .map(|row| KnowledgeListRow::from_query_result(row, ""))
            .collect()
    }
}

#[derive(DeriveIden)]
enum KnowledgePage {
    Number,
    Position,
    Table,
}

fn latest_rows(space: &str) -> SelectStatement {
    Query::select()
        .columns([Column::Number, Column::Name].map(|column| (Entity, column)))
        .column((VersionEntity, VersionColumn::Summary))
        .from(Entity)
        .join(
            JoinType::InnerJoin,
            VersionEntity,
            Condition::all()
                .add(
                    Expr::col((Entity, Column::Id))
                        .eq(Expr::col((VersionEntity, VersionColumn::KnowledgeId))),
                )
                .add(
                    Expr::col((Entity, Column::LatestVersion))
                        .eq(Expr::col((VersionEntity, VersionColumn::Version))),
                ),
        )
        .and_where(Expr::col((Entity, Column::SpaceId)).eq(space))
        .and_where(Expr::col((Entity, Column::DeletedAt)).is_null())
        .to_owned()
}

fn grouped_page(backend: DbBackend, request: &KnowledgeListRequest<'_>) -> SelectStatement {
    let position = collated(backend, level_position(backend, request.prefix));
    let mut query = Query::select();
    query
        .from(Entity)
        .expr_as(position.clone(), KnowledgePage::Position)
        .expr_as(
            Func::min(Expr::col((Entity, Column::Number))),
            KnowledgePage::Number,
        )
        .and_where(Expr::col((Entity, Column::SpaceId)).eq(request.space))
        .and_where(Expr::col((Entity, Column::DeletedAt)).is_null());
    filter_page(&mut query, backend, request);
    // Postgres compares parameter identities in grouping expressions, so reuse the selected alias.
    query
        .group_by_col(KnowledgePage::Position)
        .order_by_expr(Expr::col(KnowledgePage::Position), Order::Asc)
        .limit(request.limit)
        .to_owned()
}

fn filter_page(
    query: &mut SelectStatement,
    backend: DbBackend,
    request: &KnowledgeListRequest<'_>,
) {
    let name = collated(backend, Expr::col((Entity, Column::NameKey)));
    if let Some(prefix) = request.prefix {
        query
            .and_where(name.clone().gte(format!("{prefix}/")))
            .and_where(name.clone().lt(format!("{prefix}0")));
    }
    if let Some(after) = request.after {
        let condition = if let Some(prefix) = after.strip_suffix('/') {
            name.gte(format!("{prefix}0"))
        } else {
            name.gt(after)
        };
        query.and_where(condition);
    }
}

fn level_position(backend: DbBackend, prefix: Option<&str>) -> Expr {
    let leading = prefix.map_or(0, |prefix| prefix.chars().count() + 1) as i32;
    let name = Expr::col((Entity, Column::NameKey));
    let rest: Expr = Func::cust(Alias::new("substr"))
        .args([name.clone(), Expr::value(leading + 1)])
        .into();
    let function = if backend == DbBackend::Postgres {
        "strpos"
    } else {
        "instr"
    };
    let slash: Expr = Func::cust(Alias::new(function))
        .args([rest, Expr::value("/")])
        .into();
    let directory = Func::cust(Alias::new("substr")).args([
        name.clone(),
        Expr::value(1),
        Expr::value(leading).add(slash.clone()),
    ]);
    Expr::case(slash.gt(0), directory).finally(name).into()
}

fn keyword_match(backend: DbBackend, keyword: &str) -> Condition {
    let keyword = if backend == DbBackend::Postgres {
        keyword.to_ascii_lowercase()
    } else {
        keyword.to_owned()
    };
    let escaped = keyword
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = LikeExpr::new(format!("%{escaped}%")).escape('\\');
    let columns = [
        (Entity, Column::Name).into_column_ref(),
        (VersionEntity, VersionColumn::Summary).into_column_ref(),
        (VersionEntity, VersionColumn::Content).into_column_ref(),
    ];
    columns
        .into_iter()
        .fold(Condition::any(), |condition, column| {
            let expression = Expr::col(column);
            let expression = if backend == DbBackend::Postgres {
                Func::cust(Alias::new("translate"))
                    .args([
                        expression,
                        Expr::value("ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
                        Expr::value("abcdefghijklmnopqrstuvwxyz"),
                    ])
                    .into()
            } else {
                expression
            };
            condition.add(collated(backend, expression).like(pattern.clone()))
        })
}

#[expect(
    clippy::disallowed_methods,
    reason = "sea-query has no expression-level collation builder"
)]
fn collated(backend: DbBackend, expression: Expr) -> Expr {
    if backend == DbBackend::Postgres {
        Expr::cust_with_expr("$1 COLLATE \"C\"", expression)
    } else {
        expression
    }
}
