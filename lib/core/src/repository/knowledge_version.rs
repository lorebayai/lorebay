use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter};
use typed_builder::TypedBuilder;

use crate::entity::space_knowledge_version::{ActiveModel, Column, Entity, Model};

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct KnowledgeVersionRepository<'a, C> {
    pub connection: &'a C,
}

impl<C: ConnectionTrait> KnowledgeVersionRepository<'_, C> {
    pub async fn by_number(&self, entry: &str, version: i64) -> Result<Model, DbErr> {
        Entity::find()
            .filter(Column::KnowledgeId.eq(entry))
            .filter(Column::Version.eq(version))
            .one(self.connection)
            .await?
            .ok_or_else(|| DbErr::Type("Knowledge version is missing".to_owned()))
    }

    pub async fn insert(&self, model: ActiveModel) -> Result<Model, DbErr> {
        model.insert(self.connection).await
    }
}
