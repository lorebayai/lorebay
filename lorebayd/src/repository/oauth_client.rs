use sea_orm::sea_query::{Expr, Query};
use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter};
use typed_builder::TypedBuilder;

use crate::entity::oauth_client::{ActiveModel, Column, Entity, Model};
use crate::entity::oauth_code;

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

    pub(crate) async fn mark_authorized(&self, client_id: &str, now: i64) -> Result<(), DbErr> {
        Entity::update_many()
            .col_expr(Column::FirstAuthorizedAt, Expr::value(now))
            .filter(Column::ClientId.eq(client_id))
            .filter(Column::FirstAuthorizedAt.is_null())
            .exec(self.connection)
            .await?;
        Ok(())
    }

    pub(crate) async fn delete_unused(&self, created_before: i64, now: i64) -> Result<u64, DbErr> {
        let pending_clients = Query::select()
            .column(oauth_code::Column::ClientId)
            .from(oauth_code::Entity)
            .and_where(oauth_code::Column::ExpiresAt.gt(now))
            .and_where(oauth_code::Column::ExchangedAt.is_null())
            .to_owned();
        Ok(Entity::delete_many()
            .filter(Column::FirstAuthorizedAt.is_null())
            .filter(Column::CreatedAt.lt(created_before))
            .filter(Column::ClientId.not_in_subquery(pending_clients))
            .exec(self.connection)
            .await?
            .rows_affected)
    }
}
