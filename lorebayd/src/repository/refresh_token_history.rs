use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter};
use typed_builder::TypedBuilder;

use crate::entity::refresh_token_history::{ActiveModel, Column, Entity, Model};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct RefreshTokenHistoryRepository<'a, C> {
    pub connection: &'a C,
}

impl<C: ConnectionTrait> RefreshTokenHistoryRepository<'_, C> {
    pub async fn by_hash(&self, hash: &str, now: i64) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::TokenHash.eq(hash))
            .filter(Column::ExpiresAt.gt(now))
            .one(self.connection)
            .await
    }

    pub async fn insert(&self, model: ActiveModel) -> Result<(), DbErr> {
        model.insert(self.connection).await?;
        Ok(())
    }

    pub async fn delete_expired(&self, now: i64) -> Result<u64, DbErr> {
        Ok(Entity::delete_many()
            .filter(Column::ExpiresAt.lte(now))
            .exec(self.connection)
            .await?
            .rows_affected)
    }
}
