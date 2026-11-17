use sea_orm::{ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter};
use typed_builder::TypedBuilder;

use crate::entity::agent_authorization::{Column, Entity};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct AgentAuthorizationRepository<'a, C> {
    pub connection: &'a C,
}

impl<C: ConnectionTrait> AgentAuthorizationRepository<'_, C> {
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
