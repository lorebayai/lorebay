use sea_orm::sea_query::{Expr, ExprTrait, Query};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, DbErr, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
};
use typed_builder::TypedBuilder;

use crate::entity::space_agent_file::{ActiveModel, Column, Entity, Model};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct AgentFileRepository<'a, C> {
    pub connection: &'a C,
}

impl<C: ConnectionTrait> AgentFileRepository<'_, C> {
    pub async fn delete_abandoned(&self, before: i64, limit: u64) -> Result<Vec<String>, DbErr> {
        let candidates = Query::select()
            .column(Column::Id)
            .from(Entity)
            .and_where(Column::CompletedAt.is_null())
            .and_where(Column::RefusedAt.is_null())
            .and_where(Column::CreatedAt.lt(before))
            .order_by(Column::Id, sea_orm::Order::Asc)
            .limit(limit)
            .to_owned();
        self.delete_returning(
            Expr::col(Column::Id)
                .in_subquery(candidates)
                .and(Column::CompletedAt.is_null())
                .and(Column::RefusedAt.is_null())
                .and(Column::CreatedAt.lt(before)),
        )
        .await
    }

    pub async fn referenced_locations(&self, locations: Vec<String>) -> Result<Vec<String>, DbErr> {
        Entity::find()
            .select_only()
            .column(Column::StorageKey)
            .filter(Column::StorageKey.is_in(locations))
            .filter(Column::RefusedAt.is_null())
            .into_tuple()
            .all(self.connection)
            .await
    }

    pub async fn completed_by_storage_key(&self, location: &str) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::StorageKey.eq(location))
            .filter(Column::CompletedAt.is_not_null())
            .one(self.connection)
            .await
    }

    pub async fn completed_by_space_number(
        &self,
        space_id: &str,
        number: i64,
    ) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::SpaceId.eq(space_id))
            .filter(Column::Number.eq(number))
            .filter(Column::CompletedAt.is_not_null())
            .one(self.connection)
            .await
    }

    pub async fn by_number(
        &self,
        task_id: &str,
        number: i64,
        lock: bool,
    ) -> Result<Option<Model>, DbErr> {
        let query = Entity::find()
            .filter(Column::TaskId.eq(task_id))
            .filter(Column::Number.eq(number));
        let query = if lock && self.connection.get_database_backend() == DbBackend::Postgres {
            query.lock_exclusive()
        } else {
            query
        };
        query.one(self.connection).await
    }

    pub async fn completed(&self, task_id: &str) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(Column::TaskId.eq(task_id))
            .filter(Column::CompletedAt.is_not_null())
            .order_by_asc(Column::Number)
            .all(self.connection)
            .await
    }

    pub async fn insert(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.insert(self.connection).await
    }

    pub async fn delete_task_agent_files(&self, task_id: &str) -> Result<Vec<String>, DbErr> {
        self.delete_returning(Column::TaskId.eq(task_id)).await
    }

    pub async fn delete_completed(
        &self,
        task_id: &str,
        number: i64,
    ) -> Result<Option<String>, DbErr> {
        let rows = self
            .delete_returning(
                Column::TaskId
                    .eq(task_id)
                    .and(Column::Number.eq(number))
                    .and(Column::CompletedAt.is_not_null()),
            )
            .await?;
        Ok(rows.into_iter().next())
    }

    pub async fn update(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.update(self.connection).await
    }

    async fn delete_returning(&self, condition: Expr) -> Result<Vec<String>, DbErr> {
        let rows = self
            .connection
            .query_all(
                Query::delete()
                    .from_table(Entity)
                    .and_where(condition)
                    .returning_col(Column::StorageKey),
            )
            .await?;
        rows.into_iter()
            .map(|row| row.try_get_by_index(0))
            .collect()
    }
}
