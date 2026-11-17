use sea_orm::sea_query::{Expr, ExprTrait, LikeExpr};
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

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TaskListFilter<'a> {
    pub person: Option<&'a str>,
    pub account_id: Option<&'a str>,
    pub agent: Option<&'a str>,
    pub status: Option<&'a str>,
    pub keyword: Option<&'a str>,
}

impl TaskListFilter<'_> {
    fn condition(&self) -> Condition {
        let mut condition = Condition::all();
        if let Some(person) = self.person {
            condition = condition.add(account::Column::Username.eq(person));
        }
        if let Some(account_id) = self.account_id {
            condition = condition.add(Column::AccountId.eq(account_id));
        }
        if let Some(agent) = self.agent {
            condition = condition.add(Column::AgentName.eq(agent));
        }
        if let Some(status) = self.status {
            condition = condition.add(Column::Status.eq(status));
        }
        if let Some(keyword) = self.keyword {
            let escaped = keyword
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            let pattern = LikeExpr::new(format!("%{escaped}%")).escape('\\');
            condition = condition.add(
                Condition::any()
                    .add(Expr::col((Entity, Column::Title)).like(pattern.clone()))
                    .add(Expr::col((Entity, Column::Result)).like(pattern)),
            );
        }
        condition
    }
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
        filter: &TaskListFilter<'_>,
        after: Option<&TaskAfter>,
        limit: u64,
    ) -> Result<Vec<(Model, Option<account::Model>)>, DbErr> {
        let mut query = Entity::find()
            .find_also_related(account::Entity)
            .filter(Column::SpaceId.eq(space_id))
            .filter(filter.condition());
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
            .all(self.connection)
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
}
