use sea_orm::sea_query::{Cond, Expr, ExprTrait, Order, Query, SelectStatement, UnionType};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, DeriveIden, EntityTrait,
    FromQueryResult, QueryFilter, QueryOrder, QuerySelect,
};
use typed_builder::TypedBuilder;

use crate::entity::participant::{ActiveModel, Column, Entity, Model};
use crate::entity::{account, agent_authorization};

#[derive(Clone, Debug, FromQueryResult, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ParticipantRecord {
    pub username: String,
    pub role: String,
    pub agent_name: Option<String>,
    pub kind: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ParticipantRepository<'a, C> {
    pub connection: &'a C,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct RosterAfter {
    pub username: String,
    pub kind: String,
    pub agent_name: String,
}

impl<C: ConnectionTrait> ParticipantRepository<'_, C> {
    pub async fn list(
        &self,
        space_id: &str,
        after: Option<&RosterAfter>,
        limit: u64,
    ) -> Result<Vec<ParticipantRecord>, DbErr> {
        let mut query = Query::select();
        query
            .columns([
                Roster::Username,
                Roster::Role,
                Roster::AgentName,
                Roster::Kind,
            ])
            .from_subquery(roster_query(space_id), Roster::Table);
        if let Some(position) = after {
            query.cond_where(
                Cond::any()
                    .add(Expr::col(Roster::Username).gt(&position.username))
                    .add(
                        Cond::all()
                            .add(Expr::col(Roster::Username).eq(&position.username))
                            .add(Expr::col(Roster::Kind).lt(&position.kind)),
                    )
                    .add(
                        Cond::all()
                            .add(Expr::col(Roster::Username).eq(&position.username))
                            .add(Expr::col(Roster::Kind).eq(&position.kind))
                            .add(Expr::col(Roster::AgentName).gt(&position.agent_name)),
                    ),
            );
        }
        query
            .order_by(Roster::Username, Order::Asc)
            .order_by(Roster::Kind, Order::Desc)
            .order_by(Roster::AgentName, Order::Asc)
            .limit(limit);
        ParticipantRecord::find_by_statement(self.connection.get_database_backend().build(&query))
            .all(self.connection)
            .await
    }

    pub async fn update(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.update(self.connection).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), DbErr> {
        Entity::delete_by_id(id).exec(self.connection).await?;
        Ok(())
    }

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

    pub async fn memberships(
        &self,
        account_id: &str,
        after: i64,
        limit: u64,
    ) -> Result<Vec<Model>, DbErr> {
        Entity::find()
            .filter(Column::AccountId.eq(account_id))
            .filter(Column::SpaceNumber.gt(after))
            .order_by_asc(Column::SpaceNumber)
            .limit(limit)
            .all(self.connection)
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

#[derive(DeriveIden)]
enum Roster {
    AgentName,
    Kind,
    Role,
    Table,
    Username,
}

fn roster_query(space_id: &str) -> SelectStatement {
    let mut people = Query::select();
    people
        .column((account::Entity, account::Column::Username))
        .column((Entity, Column::Role))
        .expr_as(Expr::val(Option::<String>::None), Roster::AgentName)
        .expr_as(Expr::val("human"), Roster::Kind)
        .from(Entity)
        .inner_join(
            account::Entity,
            Expr::col((Entity, Column::AccountId)).equals((account::Entity, account::Column::Id)),
        )
        .and_where(Expr::col((Entity, Column::SpaceId)).eq(space_id))
        .and_where(Expr::col((account::Entity, account::Column::DeletedAt)).is_null());
    let mut agents = Query::select();
    agents
        .column((account::Entity, account::Column::Username))
        .column((Entity, Column::Role))
        .expr_as(
            Expr::col((
                agent_authorization::Entity,
                agent_authorization::Column::Name,
            )),
            Roster::AgentName,
        )
        .expr_as(Expr::val("agent"), Roster::Kind)
        .from(Entity)
        .inner_join(
            account::Entity,
            Expr::col((Entity, Column::AccountId)).equals((account::Entity, account::Column::Id)),
        )
        .inner_join(
            agent_authorization::Entity,
            Expr::col((Entity, Column::AccountId)).equals((
                agent_authorization::Entity,
                agent_authorization::Column::AccountId,
            )),
        )
        .and_where(Expr::col((Entity, Column::SpaceId)).eq(space_id))
        .and_where(Expr::col((account::Entity, account::Column::DeletedAt)).is_null());
    people.union(UnionType::All, agents).to_owned()
}
