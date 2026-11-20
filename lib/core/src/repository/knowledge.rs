use sea_orm::sea_query::{
    Alias, Expr, ExprTrait, Func, IntoColumnRef, JoinType, LikeExpr, OnConflict, Query,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DbBackend, DbErr, EntityTrait,
    FromQueryResult, QueryFilter, QuerySelect,
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

    pub async fn list(
        &self,
        space: &str,
        prefix: Option<&str>,
        keyword: Option<&str>,
    ) -> Result<Vec<KnowledgeListRow>, DbErr> {
        let backend = self.connection.get_database_backend();
        let mut query = Query::select();
        query
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
            .and_where(Expr::col((Entity, Column::DeletedAt)).is_null());
        if let Some(prefix) = prefix {
            let column = collated(backend, Expr::col((Entity, Column::NameKey)));
            query
                .and_where(column.clone().gte(format!("{prefix}/")))
                .and_where(column.lt(format!("{prefix}0")));
        }
        if let Some(keyword) = keyword {
            query.cond_where(keyword_match(backend, keyword));
        }
        self.connection
            .query_all(&query)
            .await?
            .iter()
            .map(|row| KnowledgeListRow::from_query_result(row, ""))
            .collect()
    }
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
