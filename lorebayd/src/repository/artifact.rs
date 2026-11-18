use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use typed_builder::TypedBuilder;

use crate::entity::artifact::{ActiveModel, Column, Entity, Model};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ArtifactRepository<'a, C> {
    pub connection: &'a C,
}

impl<C: ConnectionTrait> ArtifactRepository<'_, C> {
    pub async fn abandoned(&self, before: i64, limit: u64) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(Column::CompletedAt.is_null())
            .filter(Column::CreatedAt.lt(before))
            .order_by_asc(Column::Id)
            .limit(limit)
            .all(self.connection)
            .await
    }

    pub async fn delete_ids(&self, ids: Vec<String>) -> Result<(), DbErr> {
        Entity::delete_many()
            .filter(Column::Id.is_in(ids))
            .exec(self.connection)
            .await?;
        Ok(())
    }

    pub async fn referenced_locations(&self, locations: Vec<String>) -> Result<Vec<String>, DbErr> {
        Entity::find()
            .select_only()
            .column(Column::StorageKey)
            .filter(Column::StorageKey.is_in(locations))
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

    pub async fn by_number(&self, task_id: &str, number: i64) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::TaskId.eq(task_id))
            .filter(Column::Number.eq(number))
            .one(self.connection)
            .await
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

    pub async fn storage_keys(&self, task_id: &str) -> Result<Vec<String>, DbErr> {
        Entity::find()
            .select_only()
            .column(Column::StorageKey)
            .filter(Column::TaskId.eq(task_id))
            .into_tuple()
            .all(self.connection)
            .await
    }

    pub async fn update(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.update(self.connection).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), DbErr> {
        Entity::delete_by_id(id).exec(self.connection).await?;
        Ok(())
    }
}
