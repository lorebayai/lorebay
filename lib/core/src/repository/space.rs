use sea_orm::sea_query::{Expr, ExprTrait, Query};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use typed_builder::TypedBuilder;

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

    pub async fn by_ids(&self, ids: Vec<String>) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(Column::Id.is_in(ids))
            .all(self.connection)
            .await
    }

    pub async fn take_number(&self, id: &str, counter: Column) -> Result<i64, DbErr> {
        let row = self
            .connection
            .query_one(
                Query::update()
                    .table(Entity)
                    .value(counter, Expr::col(counter).add(1))
                    .and_where(Column::Id.eq(id))
                    .and_where(counter.lt(i64::MAX))
                    .returning_col(counter),
            )
            .await?
            .ok_or_else(|| DbErr::Type("Space counter exhausted or missing".into()))?;
        let next: i64 = row.try_get_by_index(0)?;
        next.checked_sub(1)
            .ok_or_else(|| DbErr::Type("Invalid space counter".into()))
    }

    pub async fn insert(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.insert(self.connection).await
    }

    pub async fn update(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.update(self.connection).await
    }
}
