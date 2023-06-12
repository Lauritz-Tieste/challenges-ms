use std::sync::Arc;

use entity::{challenges_coding_challenges, challenges_subtasks};
use fnct::format::JsonFormatter;
use lib::{Cache, SharedState};
use poem_ext::{response, responses::ErrorResponse};
use poem_openapi::{Object, OpenApi};
use sandkasten_client::{
    schemas::programs::{BuildRunResult, RunResult},
    SandkastenClient,
};
use sea_orm::{ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{
    schemas::coding_challenges::CheckResult,
    services::judge::{Error as JudgeError, Judge},
};

mod assets;
mod challenges;
mod judge;
mod submissions;

pub struct CodingChallenges {
    pub state: Arc<SharedState>,
    pub sandkasten: SandkastenClient,
    pub judge_cache: Cache<JsonFormatter>,
    pub judge_lock: Arc<Semaphore>,
}

impl CodingChallenges {
    pub fn get_api(self) -> impl OpenApi {
        (
            assets::Api,
            challenges::Api {
                sandkasten: self.sandkasten.clone(),
                judge_cache: self.judge_cache.clone(),
            },
            judge::Api {
                sandkasten: self.sandkasten.clone(),
                judge_cache: self.judge_cache.clone(),
            },
            submissions::Api {
                state: self.state,
                sandkasten: self.sandkasten,
                judge_cache: self.judge_cache,
                judge_lock: self.judge_lock,
                reward_lock: Default::default(),
            },
        )
    }
}

async fn get_challenge(
    db: &DatabaseTransaction,
    task_id: Uuid,
    subtask_id: Uuid,
) -> Result<
    Option<(
        challenges_coding_challenges::Model,
        challenges_subtasks::Model,
    )>,
    ErrorResponse,
> {
    Ok(
        match challenges_coding_challenges::Entity::find_by_id(subtask_id)
            .find_also_related(challenges_subtasks::Entity)
            .filter(challenges_subtasks::Column::TaskId.eq(task_id))
            .one(db)
            .await?
        {
            Some((cc, Some(subtask))) => Some((cc, subtask)),
            _ => None,
        },
    )
}

async fn check_challenge(
    CheckChallenge {
        judge,
        challenge_id,
        solution_environment,
        solution_code,
        time_limit,
        memory_limit,
        static_tests,
        random_tests,
    }: CheckChallenge<'_>,
) -> Result<Result<(), CheckError>, JudgeError> {
    let examples = match judge.examples().await {
        Err(JudgeError::EvaluatorFailed(err)) => {
            return Ok(Err(CheckError::EvaluatorFailed(err)));
        }
        Err(JudgeError::InvalidOutput(err)) => {
            return Ok(Err(CheckError::InvalidOutput(err)));
        }
        x => x?,
    };
    if examples.is_empty() {
        return Ok(Err(CheckError::NoExamples));
    }

    for seed in examples
        .into_iter()
        .chain((0..static_tests).map(|x| format!("_static_{x}_{challenge_id}")))
        .chain((0..random_tests).map(|_| Uuid::new_v4().to_string()))
    {
        let result = match judge
            .get_example_checked(
                &seed,
                solution_environment,
                solution_code,
                Some(time_limit),
                Some(memory_limit),
            )
            .await
        {
            Err(JudgeError::EnvironmentNotFound) => {
                return Ok(Err(CheckError::EnvironmentNotFound));
            }
            Err(JudgeError::EvaluatorFailed(err)) => {
                return Ok(Err(CheckError::EvaluatorFailed(err)));
            }
            Err(JudgeError::InvalidOutput(err)) => {
                return Ok(Err(CheckError::InvalidOutput(err)));
            }
            x => x?,
        };
        if let Err(result) = result {
            return Ok(Err(CheckError::TestcaseFailed(CheckTestcaseError {
                seed: seed.clone(),
                result,
            })));
        }
    }

    Ok(Ok(()))
}

mod _check_error {
    use super::*;
    response!(pub CheckError = {
        /// The list of examples provided by the evaluator is empty.
        NoExamples(404, error),
        /// The solution environment does not exist.
        EnvironmentNotFound(404, error),
        /// The evaluator crashed.
        EvaluatorFailed(400, error) => BuildRunResult,
        /// The evaluator failed to produce valid output.
        InvalidOutput(400, error) => BuildRunResult,
        /// The sample solution failed on a specific test case.
        TestcaseFailed(400, error) => CheckTestcaseError,
    });
}
use _check_error::CheckError::raw as _CheckError;

struct CheckChallenge<'a> {
    judge: Judge<'a>,
    challenge_id: Uuid,
    solution_environment: &'a str,
    solution_code: &'a str,
    time_limit: u64,
    memory_limit: u64,
    static_tests: u8,
    random_tests: u8,
}

impl From<CheckError> for _CheckError::Response {
    fn from(value: CheckError) -> Self {
        match value {
            CheckError::NoExamples => _CheckError::no_examples(),
            CheckError::EnvironmentNotFound => _CheckError::environment_not_found(),
            CheckError::EvaluatorFailed(x) => _CheckError::evaluator_failed(x),
            CheckError::InvalidOutput(x) => _CheckError::invalid_output(x),
            CheckError::TestcaseFailed(x) => _CheckError::testcase_failed(x),
        }
    }
}

#[derive(Debug)]
enum CheckError {
    /// The list of examples provided by the evaluator is empty.
    NoExamples,
    /// The solution environment does not exist.
    EnvironmentNotFound,
    /// The evaluator crashed.
    EvaluatorFailed(BuildRunResult),
    /// The evaluator failed to produce valid output.
    InvalidOutput(BuildRunResult),
    /// The sample solution failed on a specific test case.
    TestcaseFailed(CheckTestcaseError),
}

#[derive(Debug, Object)]
pub struct CheckTestcaseError {
    pub seed: String,
    pub result: CheckResult<RunResult>,
}