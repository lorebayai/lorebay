use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter};
use typed_builder::TypedBuilder;

use crate::entity::participant::{ActiveModel, Column, Entity, Model};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ParticipantRepository<'a, C> {
    pub connection: &'a C,
}

impl<C: ConnectionTrait> ParticipantRepository<'_, C> {
    pub async fn by_space_and_account(
        &self,
        space_id: &str,
        account_id: &str,
    ) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::SpaceId.eq(space_id))
            .filter(Column::AccountId.eq(account_id))
            .one(self.connection)
            .await
    }

    pub async fn insert(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.insert(self.connection).await
    }

    pub async fn delete_by_accounts(&self, ids: &[String]) -> Result<(), DbErr> {
        Entity::delete_many()
            .filter(Column::AccountId.is_in(ids.iter().cloned()))
            .exec(self.connection)
            .await?;
        Ok(())
    }
}
