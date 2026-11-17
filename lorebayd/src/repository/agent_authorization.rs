use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter};
use typed_builder::TypedBuilder;

use crate::entity::agent_authorization::{ActiveModel, Column, Entity, Model};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct AgentAuthorizationRepository<'a, C> {
    pub connection: &'a C,
}

impl<C: ConnectionTrait> AgentAuthorizationRepository<'_, C> {
    pub async fn by_random_id(&self, random_id: &str) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::RandomId.eq(random_id))
            .one(self.connection)
            .await
    }

    pub async fn by_account_and_name(
        &self,
        account_id: &str,
        name: &str,
    ) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::AccountId.eq(account_id))
            .filter(Column::Name.eq(name))
            .one(self.connection)
            .await
    }

    pub async fn delete_by_previous_refresh_hash(&self, hash: &str) -> Result<u64, DbErr> {
        Ok(Entity::delete_many()
            .filter(Column::PreviousRefreshTokenHash.eq(hash))
            .exec(self.connection)
            .await?
            .rows_affected)
    }

    pub async fn by_refresh_hash(&self, hash: &str) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::RefreshTokenHash.eq(hash))
            .one(self.connection)
            .await
    }

    pub async fn insert(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.insert(self.connection).await
    }

    pub async fn update(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.update(self.connection).await
    }

    pub async fn delete(&self, id: &str, account_id: &str) -> Result<u64, DbErr> {
        Ok(Entity::delete_by_id(id)
            .filter(Column::AccountId.eq(account_id))
            .exec(self.connection)
            .await?
            .rows_affected)
    }

    pub async fn delete_expired(&self, now: i64) -> Result<u64, DbErr> {
        Ok(Entity::delete_many()
            .filter(Column::RefreshExpiresAt.lte(now))
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
