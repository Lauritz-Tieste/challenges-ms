//! `SeaORM` Entity. Generated by sea-orm-codegen 0.12.2

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "challenges_question_attempts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub question_id: Uuid,
    pub user_id: Uuid,
    pub timestamp: DateTime,
    pub solved: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::challenges_questions::Entity",
        from = "Column::QuestionId",
        to = "super::challenges_questions::Column::SubtaskId",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    ChallengesQuestions,
}

impl Related<super::challenges_questions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ChallengesQuestions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
