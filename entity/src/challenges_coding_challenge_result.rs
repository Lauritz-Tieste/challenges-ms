//! `SeaORM` Entity. Generated by sea-orm-codegen 0.11.3

use super::sea_orm_active_enums::ChallengesVerdict;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "challenges_coding_challenge_result")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub submission_id: Uuid,
    pub verdict: ChallengesVerdict,
    #[sea_orm(column_type = "Text", nullable)]
    pub reason: Option<String>,
    pub build_status: Option<i32>,
    #[sea_orm(column_type = "Text", nullable)]
    pub build_stderr: Option<String>,
    pub build_time: Option<i32>,
    pub build_memory: Option<i32>,
    pub run_status: Option<i32>,
    #[sea_orm(column_type = "Text", nullable)]
    pub run_stderr: Option<String>,
    pub run_time: Option<i32>,
    pub run_memory: Option<i32>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::challenges_coding_challenge_submissions::Entity",
        from = "Column::SubmissionId",
        to = "super::challenges_coding_challenge_submissions::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    ChallengesCodingChallengeSubmissions,
}

impl Related<super::challenges_coding_challenge_submissions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ChallengesCodingChallengeSubmissions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
