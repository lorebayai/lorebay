use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use typed_builder::TypedBuilder;

use crate::entity::participant;
use crate::entity::space::{ActiveModel, Column, Entity, Model};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct SpaceRepository<'a, C> {
    pub connection: &'a C,
}

impl<C: ConnectionTrait> SpaceRepository<'_, C> {
    pub async fn highest_number(&self) -> Result<Option<i64>, DbErr> {
        Entity::find()
            .select_only()
            .column(Column::Number)
            .order_by_desc(Column::Number)
            .into_tuple()
            .one(self.connection)
            .await
    }

    pub async fn by_id(&self, id: &str) -> Result<Option<Model>, DbErr> {
        Entity::find_by_id(id).one(self.connection).await
    }

    pub async fn by_number(&self, number: i64) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::Number.eq(number))
            .one(self.connection)
            .await
    }

    pub async fn list_by_account(
        &self,
        account_id: &str,
        after: i64,
        limit: u64,
    ) -> Result<Vec<(Model, Option<participant::Model>)>, DbErr> {
        Entity::find()
            .find_also_related(participant::Entity)
            .filter(participant::Column::AccountId.eq(account_id))
            .filter(Column::Number.gt(after))
            .order_by_asc(Column::Number)
            .limit(limit)
            .all(self.connection)
            .await
    }

    pub async fn insert(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.insert(self.connection).await
    }

    pub async fn update(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.update(self.connection).await
    }
}
