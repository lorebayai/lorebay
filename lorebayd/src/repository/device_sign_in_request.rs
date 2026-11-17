use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter};
use typed_builder::TypedBuilder;

use crate::entity::device_sign_in_request::{ActiveModel, Column, Entity, Model};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct DeviceSignInRequestRepository<'a, C> {
    pub connection: &'a C,
}

impl<C: ConnectionTrait> DeviceSignInRequestRepository<'_, C> {
    pub async fn by_device_hash(&self, hash: &str) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::DeviceCodeHash.eq(hash))
            .one(self.connection)
            .await
    }

    pub async fn by_user_hash(&self, hash: &str) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::UserCodeHash.eq(hash))
            .one(self.connection)
            .await
    }

    pub async fn insert(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.insert(self.connection).await
    }

    pub async fn update(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.update(self.connection).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), DbErr> {
        Entity::delete_by_id(id).exec(self.connection).await?;
        Ok(())
    }

    pub async fn delete_expired(&self, now: i64) -> Result<u64, DbErr> {
        Ok(Entity::delete_many()
            .filter(Column::ExpiresAt.lte(now))
            .exec(self.connection)
            .await?
            .rows_affected)
    }

    pub async fn delete_by_accounts(&self, ids: &[String]) -> Result<(), DbErr> {
        Entity::delete_many()
            .filter(Column::AccountId.is_in(ids.iter().cloned()))
            .exec(self.connection)
            .await?;
        Ok(())
    }
}
