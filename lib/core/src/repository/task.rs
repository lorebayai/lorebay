use sea_orm::sea_query::{Alias, Expr, ExprTrait, Func, LikeExpr};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DbBackend, DbErr, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect,
};
use typed_builder::TypedBuilder;

use crate::entity::space_task::{ActiveModel, Column, Entity, Model};

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
    pub person_id: Option<&'a str>,
    pub agent: Option<&'a str>,
    pub state: Option<&'a str>,
    pub own_open: bool,
    pub keyword: Option<&'a str>,
}

impl TaskListFilter<'_> {
    fn condition(&self, backend: DbBackend) -> Condition {
        let mut condition = Condition::all();
        if let Some(person_id) = self.person_id {
            condition = condition.add(Column::PersonId.eq(person_id));
        }
        if let Some(agent) = self.agent {
            condition = condition.add(Column::AgentName.eq(agent));
        }
        if let Some(state) = self.state {
            condition = condition.add(Column::State.eq(state));
        }
        if self.own_open {
            condition = condition.add(Column::State.is_in(["working", "waiting"]));
        }
        if let Some(keyword) = self.keyword {
            condition = condition.add(keyword_match(backend, keyword));
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

    pub async fn by_number(&self, space_id: &str, number: i64) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::SpaceId.eq(space_id))
            .filter(Column::Number.eq(number))
            .one(self.connection)
            .await
    }

    pub async fn list(
        &self,
        space_id: &str,
        filter: &TaskListFilter<'_>,
        after: Option<&TaskAfter>,
        limit: u64,
    ) -> Result<Vec<Model>, DbErr> {
        let mut query = Entity::find()
            .filter(Column::SpaceId.eq(space_id))
            .filter(filter.condition(self.connection.get_database_backend()));
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

    pub async fn lock_by_id(&self, id: &str) -> Result<Option<Model>, DbErr> {
        let query = Entity::find_by_id(id);
        let query = if self.connection.get_database_backend() == DbBackend::Postgres {
            query.lock_exclusive()
        } else {
            query
        };
        query.one(self.connection).await
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

fn keyword_match(backend: DbBackend, keyword: &str) -> Condition {
    let keyword = if backend == DbBackend::Postgres {
        keyword.to_ascii_lowercase()
    } else {
        keyword.to_owned()
    };
    let escaped = keyword
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = LikeExpr::new(format!("%{escaped}%")).escape('\\');
    let title = keyword_column(backend, Column::Title);
    let result = keyword_column(backend, Column::Result);
    Condition::any()
        .add(title.like(pattern.clone()))
        .add(result.like(pattern))
}

#[expect(
    clippy::disallowed_methods,
    reason = "sea-query has no expression-level collation builder"
)]
fn keyword_column(backend: DbBackend, column: Column) -> Expr {
    let column = Expr::col((Entity, column));
    if backend != DbBackend::Postgres {
        return column;
    }
    let folded = Func::cust(Alias::new("translate")).args([
        column,
        Expr::value("ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
        Expr::value("abcdefghijklmnopqrstuvwxyz"),
    ]);
    Expr::cust_with_expr("$1 COLLATE \"C\"", folded)
}
