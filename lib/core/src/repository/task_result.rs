use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use typed_builder::TypedBuilder;

use crate::entity::space_task_result::{ActiveModel, Column, Entity, Model};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TaskResultRepository<'a, C> {
    pub connection: &'a C,
}

impl<C: ConnectionTrait> TaskResultRepository<'_, C> {
    pub async fn insert(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.insert(self.connection).await
    }

    pub async fn list(
        &self,
        task_id: &str,
        before: Option<i64>,
        limit: u64,
    ) -> Result<Vec<Model>, DbErr> {
        let mut query = Entity::find().filter(Column::TaskId.eq(task_id));
        if let Some(before) = before {
            query = query.filter(Column::Version.lt(before));
        }
        query
            .order_by_desc(Column::Version)
            .limit(limit)
            .all(self.connection)
            .await
    }
}
