use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use typed_builder::TypedBuilder;

use crate::entity::account::{ActiveModel, Column, Entity, Model};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct AccountRepository<'a, C> {
    pub connection: &'a C,
}

impl<C: ConnectionTrait> AccountRepository<'_, C> {
    pub async fn list_active(&self, after: Option<&str>, limit: u64) -> Result<Vec<Model>, DbErr> {
        let mut query = Entity::find().filter(Column::DeletedAt.is_null());
        if let Some(username) = after {
            query = query.filter(Column::Username.gt(username));
        }
        query
            .order_by_asc(Column::Username)
            .limit(limit)
            .all(self.connection)
            .await
    }

    pub async fn all(&self) -> Result<Vec<Model>, DbErr> {
        Entity::find().all(self.connection).await
    }

    pub async fn by_id(&self, id: &str) -> Result<Option<Model>, DbErr> {
        Entity::find_by_id(id).one(self.connection).await
    }

    pub async fn by_username(&self, username: &str) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::Username.eq(username))
            .one(self.connection)
            .await
    }

    pub async fn insert_many(&self, models: Vec<ActiveModel>) -> Result<(), DbErr> {
        if !models.is_empty() {
            Entity::insert_many(models).exec(self.connection).await?;
        }
        Ok(())
    }

    pub async fn set_deleted_at(&self, ids: &[String], time: Option<i64>) -> Result<(), DbErr> {
        Entity::update_many()
            .col_expr(Column::DeletedAt, Expr::value(time))
            .filter(Column::Id.is_in(ids.iter().cloned()))
            .exec(self.connection)
            .await?;
        Ok(())
    }
}
