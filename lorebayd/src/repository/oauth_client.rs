use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter};
use typed_builder::TypedBuilder;

use crate::entity::oauth_client::{ActiveModel, Column, Entity, Model};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct OAuthClientRepository<'a, C> {
    connection: &'a C,
}

impl<'a, C: ConnectionTrait> OAuthClientRepository<'a, C> {
    pub(crate) fn new(connection: &'a C) -> Self {
        Self { connection }
    }

    pub(crate) async fn insert(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.insert(self.connection).await
    }

    pub(crate) async fn by_client_id(&self, client_id: &str) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::ClientId.eq(client_id))
            .one(self.connection)
            .await
    }
}
