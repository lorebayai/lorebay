use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, FromQueryResult, JoinType,
    QueryFilter, QueryOrder, QuerySelect, RelationTrait,
};
use typed_builder::TypedBuilder;

use crate::entity::space_knowledge_version::{ActiveModel, Column, Entity, Model, Relation};
use crate::entity::space_proposal::Column as ProposalColumn;

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct KnowledgeVersionRepository<'a, C> {
    pub connection: &'a C,
}

#[derive(Clone, Debug, FromQueryResult, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct KnowledgeVersionRow {
    pub version: i64,
    pub kind: String,
    pub applied_by: String,
    pub created_at: i64,
    pub proposal_number: Option<i64>,
}

impl<C: ConnectionTrait> KnowledgeVersionRepository<'_, C> {
    pub async fn by_number(&self, entry: &str, version: i64) -> Result<Model, DbErr> {
        self.find_by_number(entry, version)
            .await?
            .ok_or_else(|| DbErr::Type("Knowledge version is missing".to_owned()))
    }

    pub async fn find_by_number(&self, entry: &str, version: i64) -> Result<Option<Model>, DbErr> {
        Entity::find()
            .filter(Column::KnowledgeId.eq(entry))
            .filter(Column::Version.eq(version))
            .one(self.connection)
            .await
    }

    pub async fn insert(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.insert(self.connection).await
    }

    pub async fn list(
        &self,
        entry: &str,
        before: Option<i64>,
        limit: u64,
    ) -> Result<Vec<KnowledgeVersionRow>, DbErr> {
        let mut query = Entity::find()
            .select_only()
            .columns([
                Column::Version,
                Column::Kind,
                Column::AppliedBy,
                Column::CreatedAt,
            ])
            .column_as(ProposalColumn::Number, "proposal_number")
            .join(JoinType::LeftJoin, Relation::SpaceProposal.def())
            .filter(Column::KnowledgeId.eq(entry));
        if let Some(before) = before {
            query = query.filter(Column::Version.lt(before));
        }
        query
            .order_by_desc(Column::Version)
            .limit(limit)
            .into_model()
            .all(self.connection)
            .await
    }
}
