use chrono::{DateTime, Utc};
use entity::{
    challenges_coding_challenge_example, challenges_coding_challenges, challenges_subtasks,
};
use poem_ext::patch_value::PatchValue;
use poem_openapi::Object;
use uuid::Uuid;

#[derive(Debug, Clone, Object)]
pub struct CodingChallenge {
    /// The unique identifier of the subtask.
    pub id: Uuid,
    /// The parent task.
    pub task_id: Uuid,
    /// The creator of the subtask
    pub creator: Uuid,
    /// The creation timestamp of the subtask
    pub creation_timestamp: DateTime<Utc>,
    /// The number of xp a user gets for completing this subtask.
    pub xp: i64,
    /// The number of morphcoins a user gets for completing this subtask.
    pub coins: i64,
    /// The challenge description.
    pub description: String,
    /// The number of milliseconds the solution may run.
    pub time_limit: u16,
    /// The number of megabytes of memory the solution may use.
    pub memory_limit: u16,
}

#[derive(Debug, Clone, Object)]
pub struct Example {
    /// The unique identifier of the example.
    pub id: Uuid,
    /// The coding challenge subtask.
    pub challenge_id: Uuid,
    /// The input the program receives via stdin.
    #[oai(validator(max_length = 4096))]
    pub input: String,
    /// The output the program should produce on stdout.
    #[oai(validator(max_length = 4096))]
    pub output: String,
    /// An optional explanation for the example.
    #[oai(validator(max_length = 4096))]
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Object)]
pub struct CreateCodingChallengeRequest {
    /// The number of xp a user gets for completing this subtask.
    pub xp: i64,
    /// The number of morphcoins a user gets for completing this subtask.
    pub coins: i64,
    /// The challenge description.
    #[oai(validator(max_length = 16384))]
    pub description: String,
    /// The number of milliseconds the solution may run.
    pub time_limit: u64,
    /// The number of megabytes of memory the solution may use.
    pub memory_limit: u64,
    /// A list of example inputs and outputs.
    #[oai(validator(max_items = 8))]
    pub examples: Vec<Example>,
    /// The program used to generate test cases and evaluate solutions
    #[oai(validator(max_length = 65536))]
    pub evaluator: String,
}

#[derive(Debug, Clone, Object)]
pub struct UpdateCodingChallengeRequest {
    /// The parent task.
    pub task_id: PatchValue<Uuid>,
    /// The number of xp a user gets for completing this subtask.
    pub xp: PatchValue<i64>,
    /// The number of morphcoins a user gets for completing this subtask.
    pub coins: PatchValue<i64>,
    /// The challenge description.
    #[oai(validator(max_length = 16384))]
    pub description: PatchValue<String>,
    /// The number of milliseconds the solution may run.
    pub time_limit: PatchValue<u64>,
    /// The number of megabytes of memory the solution may use.
    pub memory_limit: PatchValue<u64>,
    /// The program used to generate test cases and evaluate solutions
    #[oai(validator(max_length = 65536))]
    pub evaluator: PatchValue<String>,
}

#[derive(Debug, Clone, Object)]
pub struct CreateExampleRequest {
    /// The input the program receives via stdin.
    #[oai(validator(max_length = 4096))]
    pub input: String,
    /// The output the program should produce on stdout.
    #[oai(validator(max_length = 4096))]
    pub output: String,
    /// An optional explanation for the example.
    #[oai(validator(max_length = 4096))]
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Object)]
pub struct UpdateExampleRequest {
    /// The input the program receives via stdin.
    #[oai(validator(max_length = 4096))]
    pub input: PatchValue<String>,
    /// The output the program should produce on stdout.
    #[oai(validator(max_length = 4096))]
    pub output: PatchValue<String>,
    /// An optional explanation for the example.
    #[oai(validator(max_length = 4096))]
    pub explanation: PatchValue<Option<String>>,
}

impl CodingChallenge {
    pub fn from(
        cc: challenges_coding_challenges::Model,
        subtask: challenges_subtasks::Model,
    ) -> Self {
        Self {
            id: subtask.id,
            task_id: subtask.task_id,
            creator: subtask.creator,
            creation_timestamp: subtask.creation_timestamp.and_local_timezone(Utc).unwrap(),
            xp: subtask.xp,
            coins: subtask.coins,
            description: cc.description,
            time_limit: cc.time_limit as _,
            memory_limit: cc.memory_limit as _,
        }
    }
}

impl From<challenges_coding_challenge_example::Model> for Example {
    fn from(value: challenges_coding_challenge_example::Model) -> Self {
        Self {
            id: value.id,
            challenge_id: value.challenge_id,
            input: value.input,
            output: value.output,
            explanation: value.explanation,
        }
    }
}