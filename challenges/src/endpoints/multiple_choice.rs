use std::sync::Arc;

use chrono::Utc;
use entity::{
    challenges_multiple_choice_attempts, challenges_multiple_choice_quizes, challenges_subtasks,
    challenges_tasks,
};
use lib::{
    auth::{AdminAuth, VerifiedUserAuth},
    config::Config,
    SharedState,
};
use poem::web::Data;
use poem_ext::{db::DbTxn, patch_value::PatchValue, response, responses::ErrorResponse};
use poem_openapi::{param::Path, payload::Json, OpenApi};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseTransaction, EntityTrait, ModelTrait, QueryFilter,
    QueryOrder, Set, Unchanged,
};
use uuid::Uuid;

use crate::schemas::multiple_choice::{
    check_answers, split_answers, Answer, CreateMultipleChoiceQuestionRequest,
    MultipleChoiceQuestion, SolveQuestionFeedback, SolveQuestionRequest,
    UpdateMultipleChoiceQuestionRequest,
};

use super::Tags;

pub struct MultipleChoice {
    pub state: Arc<SharedState>,
    pub config: Arc<Config>,
}

#[OpenApi(tag = "Tags::MultipleChoice")]
impl MultipleChoice {
    /// List all multiple choice questions in a task.
    #[oai(path = "/tasks/:task_id/multiple_choice", method = "get")]
    async fn list_questions(
        &self,
        task_id: Path<Uuid>,
        db: Data<&DbTxn>,
        _auth: VerifiedUserAuth,
    ) -> ListQuestions::Response<VerifiedUserAuth> {
        ListQuestions::ok(
            challenges_multiple_choice_quizes::Entity::find()
                .find_also_related(challenges_subtasks::Entity)
                .filter(challenges_subtasks::Column::TaskId.eq(task_id.0))
                .order_by_asc(challenges_subtasks::Column::CreationTimestamp)
                .all(&***db)
                .await?
                .into_iter()
                .filter_map(|(mcq, subtask)| {
                    Some(MultipleChoiceQuestion::<String>::from(mcq, subtask?))
                })
                .collect(),
        )
    }

    /// Get a multiple choice question by id.
    #[oai(path = "/tasks/:task_id/multiple_choice/:subtask_id", method = "get")]
    async fn get_question(
        &self,
        task_id: Path<Uuid>,
        subtask_id: Path<Uuid>,
        db: Data<&DbTxn>,
        _auth: VerifiedUserAuth,
    ) -> GetQuestion::Response<VerifiedUserAuth> {
        match get_question(&db, task_id.0, subtask_id.0).await? {
            Some((mcq, subtask)) => {
                GetQuestion::ok(MultipleChoiceQuestion::<String>::from(mcq, subtask))
            }
            None => GetQuestion::subtask_not_found(),
        }
    }

    /// Get a multiple choice question and its solution by id.
    #[oai(
        path = "/tasks/:task_id/multiple_choice/:subtask_id/solution",
        method = "get"
    )]
    async fn get_question_with_solution(
        &self,
        task_id: Path<Uuid>,
        subtask_id: Path<Uuid>,
        db: Data<&DbTxn>,
        _auth: AdminAuth,
    ) -> GetQuestionWithSolution::Response<AdminAuth> {
        match get_question(&db, task_id.0, subtask_id.0).await? {
            Some((mcq, subtask)) => {
                GetQuestionWithSolution::ok(MultipleChoiceQuestion::<Answer>::from(mcq, subtask))
            }
            None => GetQuestionWithSolution::subtask_not_found(),
        }
    }

    /// Create a new multiple choice question.
    #[oai(path = "/tasks/:task_id/multiple_choice", method = "post")]
    async fn create_question(
        &self,
        task_id: Path<Uuid>,
        data: Json<CreateMultipleChoiceQuestionRequest>,
        db: Data<&DbTxn>,
        auth: AdminAuth,
    ) -> CreateQuestion::Response<AdminAuth> {
        let task = match get_task(&db, task_id.0).await? {
            Some(task) => task,
            None => return CreateQuestion::task_not_found(),
        };
        let subtask = challenges_subtasks::ActiveModel {
            id: Set(Uuid::new_v4()),
            task_id: Set(task.id),
            creator: Set(auth.0.id),
            creation_timestamp: Set(Utc::now().naive_utc()),
            xp: Set(data.0.xp),
            coins: Set(data.0.coins),
        }
        .insert(&***db)
        .await?;
        let (answers, correct) = split_answers(data.0.answers);
        let mcq = challenges_multiple_choice_quizes::ActiveModel {
            subtask_id: Set(subtask.id),
            question: Set(data.0.question),
            answers: Set(answers),
            correct_answers: Set(correct),
        }
        .insert(&***db)
        .await?;
        CreateQuestion::ok(MultipleChoiceQuestion::<Answer>::from(mcq, subtask))
    }

    /// Update a multiple choice question.
    #[oai(path = "/tasks/:task_id/multiple_choice/:subtask_id", method = "patch")]
    async fn update_question(
        &self,
        task_id: Path<Uuid>,
        subtask_id: Path<Uuid>,
        data: Json<UpdateMultipleChoiceQuestionRequest>,
        db: Data<&DbTxn>,
        _auth: AdminAuth,
    ) -> UpdateQuestion::Response<AdminAuth> {
        match get_question(&db, task_id.0, subtask_id.0).await? {
            Some((mcq, subtask)) => {
                if get_task(&db, *data.0.task_id.get_new(&subtask.task_id))
                    .await?
                    .is_none()
                {
                    return UpdateQuestion::task_not_found();
                };
                let (answers, correct) = if let PatchValue::Set(answers) = data.0.answers {
                    let (a, c) = split_answers(answers);
                    (Set(a), Set(c))
                } else {
                    (Unchanged(mcq.answers), Unchanged(mcq.correct_answers))
                };
                let mcq = challenges_multiple_choice_quizes::ActiveModel {
                    subtask_id: Unchanged(mcq.subtask_id),
                    question: data.0.question.update(mcq.question),
                    answers,
                    correct_answers: correct,
                }
                .update(&***db)
                .await?;
                let subtask = challenges_subtasks::ActiveModel {
                    id: Unchanged(subtask.id),
                    task_id: data.0.task_id.update(subtask.task_id),
                    creator: Unchanged(subtask.creator),
                    creation_timestamp: Unchanged(subtask.creation_timestamp),
                    xp: data.0.xp.update(subtask.xp),
                    coins: data.0.coins.update(subtask.coins),
                }
                .update(&***db)
                .await?;
                UpdateQuestion::ok(MultipleChoiceQuestion::<Answer>::from(mcq, subtask))
            }
            None => UpdateQuestion::subtask_not_found(),
        }
    }

    /// Delete a multiple choice question.
    #[oai(
        path = "/tasks/:task_id/multiple_choice/:subtask_id",
        method = "delete"
    )]
    async fn delete_question(
        &self,
        task_id: Path<Uuid>,
        subtask_id: Path<Uuid>,
        db: Data<&DbTxn>,
        _auth: AdminAuth,
    ) -> DeleteQuestion::Response<AdminAuth> {
        match get_question(&db, task_id.0, subtask_id.0).await? {
            Some((_, subtask)) => {
                subtask.delete(&***db).await?;
                DeleteQuestion::ok()
            }
            None => DeleteQuestion::subtask_not_found(),
        }
    }

    /// Attempt to solve a multiple choice question.
    #[oai(
        path = "/tasks/:task_id/multiple_choice/:subtask_id/attempts",
        method = "post"
    )]
    async fn solve_question(
        &self,
        task_id: Path<Uuid>,
        subtask_id: Path<Uuid>,
        data: Json<SolveQuestionRequest>,
        db: Data<&DbTxn>,
        auth: VerifiedUserAuth,
    ) -> SolveQuestion::Response<VerifiedUserAuth> {
        let Some((mcq, _)) = get_question(&db, task_id.0, subtask_id.0).await? else {
            return SolveQuestion::subtask_not_found();
        };
        if data.0.answers.len() != mcq.answers.len() {
            return SolveQuestion::wrong_length();
        }

        let previous_attempts = mcq
            .find_related(challenges_multiple_choice_attempts::Entity)
            .filter(challenges_multiple_choice_attempts::Column::UserId.eq(auth.0.id))
            .order_by_desc(challenges_multiple_choice_attempts::Column::Timestamp)
            .all(&***db)
            .await?;
        let solved_previously = previous_attempts.iter().any(|a| a.solved);
        if let Some(last_attempt) = previous_attempts.first() {
            let time_left = self
                .config
                .challenges
                .multiple_choice_questions
                .timeout_incr as i64
                * previous_attempts.len() as i64
                - (Utc::now().naive_utc() - last_attempt.timestamp).num_seconds();
            if !solved_previously && time_left > 0 {
                return SolveQuestion::too_many_requests(time_left as u64);
            }
        }

        let correct = check_answers(&data.0.answers, mcq.correct_answers);
        let solved = correct == mcq.answers.len();

        if !solved_previously {
            if solved {
                // TODO send coins and xp to user
                tracing::debug!("sending coins and xp to {}", auth.0.id);
            }

            challenges_multiple_choice_attempts::ActiveModel {
                id: Set(Uuid::new_v4()),
                question_id: Set(mcq.subtask_id),
                user_id: Set(auth.0.id),
                timestamp: Set(Utc::now().naive_utc()),
                solved: Set(solved),
            }
            .insert(&***db)
            .await?;
        }

        SolveQuestion::ok(SolveQuestionFeedback { solved, correct })
    }
}

response!(ListQuestions = {
    Ok(200) => Vec<MultipleChoiceQuestion<String>>,
});

response!(GetQuestion = {
    Ok(200) => MultipleChoiceQuestion<String>,
    /// Subtask does not exist.
    SubtaskNotFound(404, error),
});

response!(GetQuestionWithSolution = {
    Ok(200) => MultipleChoiceQuestion<Answer>,
    /// Subtask does not exist.
    SubtaskNotFound(404, error),
});

response!(CreateQuestion = {
    Ok(201) => MultipleChoiceQuestion<Answer>,
    /// Task does not exist.
    TaskNotFound(404, error),
});

response!(UpdateQuestion = {
    Ok(200) => MultipleChoiceQuestion<Answer>,
    /// Subtask does not exist.
    SubtaskNotFound(404, error),
    /// Task does not exist.
    TaskNotFound(404, error),
});

response!(DeleteQuestion = {
    Ok(200),
    /// Subtask does not exist.
    SubtaskNotFound(404, error),
});

response!(SolveQuestion = {
    Ok(201) => SolveQuestionFeedback,
    /// Wrong number of answers.
    WrongLength(422, error),
    /// Try again later. `details` contains the number of seconds to wait.
    TooManyRequests(429, error) => u64,
    /// Subtask does not exist.
    SubtaskNotFound(404, error),
});

async fn get_question(
    db: &DatabaseTransaction,
    task_id: Uuid,
    subtask_id: Uuid,
) -> Result<
    Option<(
        challenges_multiple_choice_quizes::Model,
        challenges_subtasks::Model,
    )>,
    ErrorResponse,
> {
    Ok(
        match challenges_multiple_choice_quizes::Entity::find_by_id(subtask_id)
            .find_also_related(challenges_subtasks::Entity)
            .filter(challenges_subtasks::Column::TaskId.eq(task_id))
            .one(db)
            .await?
        {
            Some((mcq, Some(subtask))) => Some((mcq, subtask)),
            _ => None,
        },
    )
}

async fn get_task(
    db: &DatabaseTransaction,
    task_id: Uuid,
) -> Result<Option<challenges_tasks::Model>, ErrorResponse> {
    Ok(challenges_tasks::Entity::find_by_id(task_id)
        .one(db)
        .await?)
}