use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DbErr, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
};
use typed_builder::TypedBuilder;

use crate::entity::account;
use crate::entity::task::{ActiveModel, Column, Entity, Model};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TaskAfter {
    pub last_activity_at: i64,
    pub number: i64,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TaskRepository<'a, C> {
    pub connection: &'a C,
}

impl<C: ConnectionTrait> TaskRepository<'_, C> {
    pub async fn by_random_id(&self, task_id: &str) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::RandomId.eq(task_id))
            .one(self.connection)
            .await
    }

    pub async fn by_number(
        &self,
        space_id: &str,
        number: i64,
    ) -> Result<Option<(Model, Option<account::Model>)>, DbErr> {
        Entity::find()
            .filter(Column::SpaceId.eq(space_id))
            .filter(Column::Number.eq(number))
            .find_also_related(account::Entity)
            .one(self.connection)
            .await
    }

    pub async fn list(
        &self,
        space_id: &str,
        after: Option<&TaskAfter>,
        limit: u64,
    ) -> Result<Vec<(Model, Option<account::Model>)>, DbErr> {
        let mut query = Entity::find().filter(Column::SpaceId.eq(space_id));
        if let Some(after) = after {
            query = query.filter(
                Condition::any()
                    .add(Column::LastActivityAt.lt(after.last_activity_at))
                    .add(
                        Condition::all()
                            .add(Column::LastActivityAt.eq(after.last_activity_at))
                            .add(Column::Number.lt(after.number)),
                    ),
            );
        }
        query
            .order_by_desc(Column::LastActivityAt)
            .order_by_desc(Column::Number)
            .limit(limit)
            .find_also_related(account::Entity)
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
