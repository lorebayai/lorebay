use sea_orm::sea_query::{Expr, ExprTrait, IntoColumnRef, JoinType, Order, Query, SelectStatement};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DbBackend, DbErr, DeriveIden,
    EntityTrait, FromQueryResult, Iterable, QueryFilter, QueryResult, QuerySelect,
};
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use crate::entity::space_person_request::{ActiveModel, Column, Entity, Model};
use crate::entity::{space_knowledge as entry, space_task as task};

use super::knowledge::keyword_condition;

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct PersonRequestRepository<'a, C> {
    pub connection: &'a C,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct PersonRequestRow {
    pub model: Model,
    pub target_name: Option<String>,
    pub target_number: Option<i64>,
    pub latest_version: Option<i64>,
    pub task_number: Option<i64>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct PersonRequestListRequest<'a> {
    pub space: &'a str,
    pub status: Option<&'a str>,
    pub keyword: Option<&'a str>,
    pub after: Option<PersonRequestAfter>,
    pub limit: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct PersonRequestAfter {
    pub created_at: i64,
    pub number: i64,
}

impl<C: ConnectionTrait> PersonRequestRepository<'_, C> {
    pub async fn insert(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.insert(self.connection).await
    }

    pub async fn update(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.update(self.connection).await
    }

    pub async fn locked(&self, space: &str, number: i64) -> Result<Option<Model>, DbErr> {
        let query = Entity::find()
            .filter(Column::SpaceId.eq(space))
            .filter(Column::Number.eq(number));
        let query = if self.connection.get_database_backend() == DbBackend::Postgres {
            query.lock_exclusive()
        } else {
            query
        };
        query.one(self.connection).await
    }

    pub async fn read(&self, space: &str, number: i64) -> Result<Option<PersonRequestRow>, DbErr> {
        let mut query = rows(space, true);
        query.and_where(Expr::col((Entity, Column::Number)).eq(number));
        self.connection
            .query_one(&query)
            .await?
            .as_ref()
            .map(row)
            .transpose()
    }

    pub async fn list(
        &self,
        request: PersonRequestListRequest<'_>,
    ) -> Result<Vec<PersonRequestRow>, DbErr> {
        let mut query = rows(request.space, false);
        if let Some(status) = request.status {
            query.and_where(Expr::col((Entity, Column::Status)).eq(status));
        }
        if let Some(after) = request.after {
            query.cond_where(
                Condition::any()
                    .add(Expr::col((Entity, Column::CreatedAt)).lt(after.created_at))
                    .add(
                        Condition::all()
                            .add(Expr::col((Entity, Column::CreatedAt)).eq(after.created_at))
                            .add(Expr::col((Entity, Column::Number)).lt(after.number)),
                    ),
            );
        }
        if let Some(keyword) = request.keyword {
            query.cond_where(keyword_condition(
                self.connection.get_database_backend(),
                keyword,
                [
                    (Entity, Column::NewName).into_column_ref(),
                    (Entity, Column::NewSummary).into_column_ref(),
                    (entry::Entity, entry::Column::Name).into_column_ref(),
                ],
            ));
        }
        query
            .order_by((Entity, Column::CreatedAt), Order::Desc)
            .order_by((Entity, Column::Number), Order::Desc)
            .limit(request.limit);
        self.connection
            .query_all(&query)
            .await?
            .iter()
            .map(row)
            .collect()
    }
}

#[derive(DeriveIden)]
enum Projection {
    LatestVersion,
    TargetName,
    TargetNumber,
    TaskNumber,
}

fn rows(space: &str, content: bool) -> SelectStatement {
    let mut query = Query::select();
    for column in Column::iter() {
        if matches!(column, Column::NewContent) && !content {
            query.expr_as(Expr::value(None::<String>), column);
        } else {
            query.column((Entity, column));
        }
    }
    query
        .from(Entity)
        .expr_as(
            Expr::col((entry::Entity, entry::Column::Name)),
            Projection::TargetName,
        )
        .expr_as(
            Expr::col((entry::Entity, entry::Column::Number)),
            Projection::TargetNumber,
        )
        .expr_as(
            Expr::col((entry::Entity, entry::Column::LatestVersion)),
            Projection::LatestVersion,
        )
        .expr_as(
            Expr::col((task::Entity, task::Column::Number)),
            Projection::TaskNumber,
        )
        .join(
            JoinType::LeftJoin,
            entry::Entity,
            Expr::col((Entity, Column::KnowledgeId))
                .eq(Expr::col((entry::Entity, entry::Column::Id))),
        )
        .join(
            JoinType::LeftJoin,
            task::Entity,
            Expr::col((Entity, Column::FromTaskId)).eq(Expr::col((task::Entity, task::Column::Id))),
        )
        .and_where(Expr::col((Entity, Column::SpaceId)).eq(space));
    query
}

fn row(row: &QueryResult) -> Result<PersonRequestRow, DbErr> {
    Ok(PersonRequestRow {
        model: Model::from_query_result(row, "")?,
        target_name: row.try_get("", "target_name")?,
        target_number: row.try_get("", "target_number")?,
        latest_version: row.try_get("", "latest_version")?,
        task_number: row.try_get("", "task_number")?,
    })
}
