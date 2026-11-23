use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use typed_builder::TypedBuilder;

use crate::entity::space_checkpoint::{ActiveModel, Column, Entity, Model};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct CheckpointRepository<'a, C> {
    pub connection: &'a C,
}

impl<C: ConnectionTrait> CheckpointRepository<'_, C> {
    pub async fn latest_time(&self, task_id: &str) -> Result<Option<i64>, DbErr> {
        Entity::find()
            .select_only()
            .column(Column::CreatedAt)
            .filter(Column::TaskId.eq(task_id))
            .order_by_desc(Column::CreatedAt)
            .into_tuple()
            .one(self.connection)
            .await
    }

    pub async fn list(
        &self,
        task_id: &str,
        after: Option<i64>,
        limit: u64,
    ) -> Result<Vec<Model>, DbErr> {
        let mut query = Entity::find().filter(Column::TaskId.eq(task_id));
        if let Some(after) = after {
            query = query.filter(Column::CreatedAt.gt(after));
        }
        query
            .order_by_asc(Column::CreatedAt)
            .limit(limit)
            .all(self.connection)
            .await
    }

    pub async fn insert(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.insert(self.connection).await
    }

    pub async fn delete_by_time(&self, task_id: &str, time: i64) -> Result<u64, DbErr> {
        let result = Entity::delete_many()
            .filter(Column::TaskId.eq(task_id))
            .filter(Column::CreatedAt.eq(time))
            .exec(self.connection)
            .await?;
        Ok(result.rows_affected)
    }
}
