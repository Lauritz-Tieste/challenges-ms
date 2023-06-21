use std::{collections::HashSet, sync::Arc};

use chrono::{DateTime, Utc};
use entity::{
    challenges_matching_attempts, challenges_matchings, challenges_subtasks,
    challenges_user_subtasks, sea_orm_active_enums::ChallengesBanAction,
};
use lib::{
    auth::{AdminAuth, VerifiedUserAuth},
    config::Config,
    SharedState,
};
use poem::web::Data;
use poem_ext::{db::DbTxn, response, responses::ErrorResponse};
use poem_openapi::{
    param::{Path, Query},
    payload::Json,
    OpenApi,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseTransaction, EntityTrait, ModelTrait, QueryFilter,
    QueryOrder, Set, Unchanged,
};
use uuid::Uuid;

use super::Tags;
use crate::{
    schemas::matchings::{
        CreateMatchingRequest, Matching, MatchingSummary, MatchingWithSolution,
        SolveMatchingFeedback, SolveMatchingRequest, UpdateMatchingRequest,
    },
    services::{
        subtasks::{
            can_create, get_active_ban, get_user_subtask, get_user_subtasks, send_task_rewards,
            update_user_subtask, ActiveBan, UserSubtaskExt,
        },
        tasks::{get_task, get_task_with_specific, Task},
    },
};

pub struct Matchings {
    pub state: Arc<SharedState>,
    pub config: Arc<Config>,
}

#[OpenApi(tag = "Tags::Matchings")]
impl Matchings {
    /// List all matchings in a task.
    #[oai(path = "/tasks/:task_id/matchings", method = "get")]
    #[allow(clippy::too_many_arguments)]
    async fn list_matchings(
        &self,
        task_id: Path<Uuid>,
        /// Whether to search for free matchings.
        free: Query<Option<bool>>,
        /// Whether to search for unlocked matchings.
        unlocked: Query<Option<bool>>,
        /// Whether to search for solved matchings.
        solved: Query<Option<bool>>,
        /// Whether to search for rated matchings.
        rated: Query<Option<bool>>,
        /// Whether to search for enabled subtasks.
        enabled: Query<Option<bool>>,
        db: Data<&DbTxn>,
        auth: VerifiedUserAuth,
    ) -> ListMatchings::Response<VerifiedUserAuth> {
        let subtasks = get_user_subtasks(&db, auth.0.id).await?;
        ListMatchings::ok(
            challenges_matchings::Entity::find()
                .find_also_related(challenges_subtasks::Entity)
                .filter(challenges_subtasks::Column::TaskId.eq(task_id.0))
                .order_by_asc(challenges_subtasks::Column::CreationTimestamp)
                .all(&***db)
                .await?
                .into_iter()
                .filter_map(|(matching, subtask)| {
                    let subtask = subtask?;
                    let id = subtask.id;
                    let free_ = subtask.fee <= 0;
                    let unlocked_ = subtasks.get(&id).check_access(&auth.0, &subtask);
                    let solved_ = subtasks.get(&id).is_solved();
                    let rated_ = subtasks.get(&id).is_rated();
                    let enabled_ = subtask.enabled;
                    ((auth.0.admin || auth.0.id == subtask.creator || subtask.enabled)
                        && free.unwrap_or(free_) == free_
                        && unlocked.unwrap_or(unlocked_) == unlocked_
                        && solved.unwrap_or(solved_) == solved_
                        && rated.unwrap_or(rated_) == rated_
                        && enabled.unwrap_or(enabled_) == enabled_)
                        .then_some(MatchingSummary::from(
                            matching, subtask, unlocked_, solved_, rated_,
                        ))
                })
                .collect(),
        )
    }

    /// Get a matching by id.
    #[oai(path = "/tasks/:task_id/matchings/:subtask_id", method = "get")]
    async fn get_matching(
        &self,
        task_id: Path<Uuid>,
        subtask_id: Path<Uuid>,
        db: Data<&DbTxn>,
        auth: VerifiedUserAuth,
    ) -> GetMatching::Response<VerifiedUserAuth> {
        let Some((matching, subtask)) = get_matching(&db, task_id.0, subtask_id.0).await? else {
            return GetMatching::subtask_not_found();
        };
        if !auth.0.admin && auth.0.id != subtask.creator && !subtask.enabled {
            return GetMatching::subtask_not_found();
        }

        let user_subtask = get_user_subtask(&db, auth.0.id, subtask.id).await?;
        if !user_subtask.check_access(&auth.0, &subtask) {
            return GetMatching::no_access();
        }

        GetMatching::ok(Matching::from(
            matching,
            subtask,
            true,
            user_subtask.is_solved(),
            user_subtask.is_rated(),
        ))
    }

    /// Get a matching and its solution by id.
    #[oai(
        path = "/tasks/:task_id/matchings/:subtask_id/solution",
        method = "get"
    )]
    async fn get_matching_with_solution(
        &self,
        task_id: Path<Uuid>,
        subtask_id: Path<Uuid>,
        db: Data<&DbTxn>,
        auth: VerifiedUserAuth,
    ) -> GetMatchingWithSolution::Response<VerifiedUserAuth> {
        let Some((matching, subtask)) = get_matching(&db, task_id.0, subtask_id.0).await? else {
            return GetMatchingWithSolution::subtask_not_found();
        };

        if !(auth.0.admin || auth.0.id == subtask.creator) {
            return GetMatchingWithSolution::forbidden();
        }

        let user_subtask = get_user_subtask(&db, auth.0.id, subtask.id).await?;
        GetMatchingWithSolution::ok(MatchingWithSolution::from(
            matching,
            subtask,
            true,
            user_subtask.is_solved(),
            user_subtask.is_rated(),
        ))
    }

    /// Create a new matching.
    #[oai(path = "/tasks/:task_id/matchings", method = "post")]
    async fn create_matching(
        &self,
        task_id: Path<Uuid>,
        data: Json<CreateMatchingRequest>,
        db: Data<&DbTxn>,
        auth: VerifiedUserAuth,
    ) -> CreateMatching::Response<VerifiedUserAuth> {
        let (task, specific) = match get_task_with_specific(&db, task_id.0).await? {
            Some(task) => task,
            None => return CreateMatching::task_not_found(),
        };
        if !can_create(&self.state.services, &self.config, &specific, &auth.0).await? {
            return CreateMatching::forbidden();
        }

        if matches!(specific, Task::CourseTask(_)) && !auth.0.admin {
            if data.0.xp > self.config.challenges.quizzes.max_xp {
                return CreateMatching::xp_limit_exceeded(self.config.challenges.quizzes.max_xp);
            }
            if data.0.coins > self.config.challenges.quizzes.max_coins {
                return CreateMatching::coin_limit_exceeded(
                    self.config.challenges.quizzes.max_coins,
                );
            }
            if data.0.fee > self.config.challenges.quizzes.max_fee {
                return CreateMatching::fee_limit_exceeded(self.config.challenges.quizzes.max_fee);
            }
        }

        match get_active_ban(&db, &auth.0, ChallengesBanAction::Create).await? {
            ActiveBan::NotBanned => {}
            ActiveBan::Temporary(end) => return CreateMatching::banned(Some(end)),
            ActiveBan::Permanent => return CreateMatching::banned(None),
        }

        match check_matching(&data.0.left, &data.0.right, &data.0.solution) {
            Ok(()) => {}
            Err(InvalidMatchingError::LeftRightDifferentLength) => {
                return CreateMatching::left_right_different_length()
            }
            Err(InvalidMatchingError::SolutionDifferentLength) => {
                return CreateMatching::solution_different_length()
            }
            Err(InvalidMatchingError::InvalidIndex(x)) => return CreateMatching::invalid_index(x),
            Err(InvalidMatchingError::RightEntriesNotMatched(x)) => {
                return CreateMatching::right_entries_not_matched(x)
            }
        }

        let subtask = challenges_subtasks::ActiveModel {
            id: Set(Uuid::new_v4()),
            task_id: Set(task.id),
            creator: Set(auth.0.id),
            creation_timestamp: Set(Utc::now().naive_utc()),
            xp: Set(data.0.xp as _),
            coins: Set(data.0.coins as _),
            fee: Set(data.0.fee as _),
            enabled: Set(true),
        }
        .insert(&***db)
        .await?;
        let matching = challenges_matchings::ActiveModel {
            subtask_id: Set(subtask.id),
            left: Set(data.0.left),
            right: Set(data.0.right),
            solution: Set(data.0.solution.into_iter().map(|x| x as _).collect()),
        }
        .insert(&***db)
        .await?;
        CreateMatching::ok(MatchingWithSolution::from(
            matching, subtask, true, false, false,
        ))
    }

    /// Update a multiple choice matching.
    #[oai(path = "/tasks/:task_id/matchings/:subtask_id", method = "patch")]
    async fn update_matching(
        &self,
        task_id: Path<Uuid>,
        subtask_id: Path<Uuid>,
        data: Json<UpdateMatchingRequest>,
        db: Data<&DbTxn>,
        auth: AdminAuth,
    ) -> UpdateMatching::Response<AdminAuth> {
        let Some((matching, subtask)) = get_matching(&db, task_id.0, subtask_id.0).await? else {
            return UpdateMatching::subtask_not_found();
        };

        if get_task(&db, *data.0.task_id.get_new(&subtask.task_id))
            .await?
            .is_none()
        {
            return UpdateMatching::task_not_found();
        };

        match check_matching(
            data.0.left.get_new(&matching.left),
            data.0.right.get_new(&matching.right),
            data.0
                .solution
                .get_new(&matching.solution.iter().map(|&x| x as _).collect()),
        ) {
            Ok(()) => {}
            Err(InvalidMatchingError::LeftRightDifferentLength) => {
                return UpdateMatching::left_right_different_length()
            }
            Err(InvalidMatchingError::SolutionDifferentLength) => {
                return UpdateMatching::solution_different_length()
            }
            Err(InvalidMatchingError::InvalidIndex(x)) => return UpdateMatching::invalid_index(x),
            Err(InvalidMatchingError::RightEntriesNotMatched(x)) => {
                return UpdateMatching::right_entries_not_matched(x)
            }
        }

        let matching = challenges_matchings::ActiveModel {
            subtask_id: Unchanged(matching.subtask_id),
            left: data.0.left.update(matching.left),
            right: data.0.right.update(matching.right),
            solution: data
                .0
                .solution
                .map(|x| x.into_iter().map(|x| x as _).collect())
                .update(matching.solution),
        }
        .update(&***db)
        .await?;
        let subtask = challenges_subtasks::ActiveModel {
            id: Unchanged(subtask.id),
            task_id: data.0.task_id.update(subtask.task_id),
            creator: Unchanged(subtask.creator),
            creation_timestamp: Unchanged(subtask.creation_timestamp),
            xp: data.0.xp.map(|x| x as _).update(subtask.xp),
            coins: data.0.coins.map(|x| x as _).update(subtask.coins),
            fee: data.0.fee.map(|x| x as _).update(subtask.fee),
            enabled: data.0.enabled.update(subtask.enabled),
        }
        .update(&***db)
        .await?;

        let user_subtask = get_user_subtask(&db, auth.0.id, subtask.id).await?;
        UpdateMatching::ok(MatchingWithSolution::from(
            matching,
            subtask,
            true,
            user_subtask.is_solved(),
            user_subtask.is_rated(),
        ))
    }

    /// Delete a multiple choice matching.
    #[oai(path = "/tasks/:task_id/matchings/:subtask_id", method = "delete")]
    async fn delete_matching(
        &self,
        task_id: Path<Uuid>,
        subtask_id: Path<Uuid>,
        db: Data<&DbTxn>,
        _auth: AdminAuth,
    ) -> DeleteMatching::Response<AdminAuth> {
        match get_matching(&db, task_id.0, subtask_id.0).await? {
            Some((_, subtask)) => {
                subtask.delete(&***db).await?;
                DeleteMatching::ok()
            }
            None => DeleteMatching::subtask_not_found(),
        }
    }

    /// Attempt to solve a multiple choice matching.
    #[oai(
        path = "/tasks/:task_id/matchings/:subtask_id/attempts",
        method = "post"
    )]
    async fn solve_matching(
        &self,
        task_id: Path<Uuid>,
        subtask_id: Path<Uuid>,
        data: Json<SolveMatchingRequest>,
        db: Data<&DbTxn>,
        auth: VerifiedUserAuth,
    ) -> SolveMatching::Response<VerifiedUserAuth> {
        let Some((matching, subtask)) = get_matching(&db, task_id.0, subtask_id.0).await? else {
                return SolveMatching::subtask_not_found();
            };
        if !auth.0.admin && auth.0.id != subtask.creator && !subtask.enabled {
            return SolveMatching::subtask_not_found();
        }

        let user_subtask = get_user_subtask(&db, auth.0.id, subtask.id).await?;
        if !user_subtask.check_access(&auth.0, &subtask) {
            return SolveMatching::no_access();
        }

        if data.0.answer.len() != matching.solution.len() {
            return SolveMatching::solution_different_length();
        }

        let previous_attempts = matching
            .find_related(challenges_matching_attempts::Entity)
            .filter(challenges_matching_attempts::Column::UserId.eq(auth.0.id))
            .order_by_desc(challenges_matching_attempts::Column::Timestamp)
            .all(&***db)
            .await?;
        let solved_previously = user_subtask.is_solved();
        if let Some(last_attempt) = previous_attempts.first() {
            let time_left = self.config.challenges.matchings.timeout_incr as i64
                * previous_attempts.len() as i64
                - (Utc::now().naive_utc() - last_attempt.timestamp).num_seconds();
            if !solved_previously && time_left > 0 {
                return SolveMatching::too_many_requests(time_left as u64);
            }
        }

        let correct = data
            .0
            .answer
            .iter()
            .zip(matching.solution.iter())
            .filter(|(&x, &y)| x == y as u8)
            .count();
        let solved = correct == matching.solution.len();

        if !solved_previously {
            let now = Utc::now().naive_utc();
            if solved {
                update_user_subtask(
                    &db,
                    user_subtask.as_ref(),
                    challenges_user_subtasks::ActiveModel {
                        user_id: Set(auth.0.id),
                        subtask_id: Set(subtask.id),
                        unlocked_timestamp: user_subtask
                            .as_ref()
                            .and_then(|x| x.unlocked_timestamp)
                            .map(|x| Unchanged(Some(x)))
                            .unwrap_or(Set(Some(now))),
                        solved_timestamp: Set(Some(now)),
                        ..Default::default()
                    },
                )
                .await?;

                if auth.0.id != subtask.creator {
                    send_task_rewards(&self.state.services, &db, auth.0.id, &subtask).await?;
                }
            }

            challenges_matching_attempts::ActiveModel {
                id: Set(Uuid::new_v4()),
                matching_id: Set(matching.subtask_id),
                user_id: Set(auth.0.id),
                timestamp: Set(now),
                solved: Set(solved),
            }
            .insert(&***db)
            .await?;
        }

        SolveMatching::ok(SolveMatchingFeedback { solved, correct })
    }
}

response!(ListMatchings = {
    Ok(200) => Vec<MatchingSummary>,
});

response!(GetMatching = {
    Ok(200) => Matching,
    /// Subtask does not exist.
    SubtaskNotFound(404, error),
    /// The user has not unlocked this matching.
    NoAccess(403, error),
});

response!(GetMatchingWithSolution = {
    Ok(200) => MatchingWithSolution,
    /// Subtask does not exist.
    SubtaskNotFound(404, error),
    /// The user is not allowed to view the solution to this matching.
    Forbidden(403, error),
});

response!(CreateMatching = {
    Ok(201) => MatchingWithSolution,
    /// Task does not exist.
    TaskNotFound(404, error),
    /// The user is not allowed to create matchings in this task.
    Forbidden(403, error),
    /// The user is currently banned from creating subtasks.
    Banned(403, error) => Option<DateTime<Utc>>,
    /// The max xp limit has been exceeded.
    XpLimitExceeded(403, error) => u64,
    /// The max coin limit has been exceeded.
    CoinLimitExceeded(403, error) => u64,
    /// The max fee limit has been exceeded.
    FeeLimitExceeded(403, error) => u64,
    /// The left list does not contain the same number of entries as the right list.
    LeftRightDifferentLength(400, error),
    /// The solution list does not contain the same number of entries as the left and right lists.
    SolutionDifferentLength(400, error),
    /// The solution list contains an invalid index.
    InvalidIndex(400, error) => u8,
    /// One or more entries in the right list have no match in the left list.
    RightEntriesNotMatched(400, error) => HashSet<u8>,
});

response!(UpdateMatching = {
    Ok(200) => MatchingWithSolution,
    /// Subtask does not exist.
    SubtaskNotFound(404, error),
    /// Task does not exist.
    TaskNotFound(404, error),
    /// The left list does not contain the same number of entries as the right list.
    LeftRightDifferentLength(400, error),
    /// The solution list does not contain the same number of entries as the left and right lists.
    SolutionDifferentLength(400, error),
    /// The solution list contains an invalid index.
    InvalidIndex(400, error) => u8,
    /// One or more entries in the right list have no match in the left list.
    RightEntriesNotMatched(400, error) => HashSet<u8>,
});

response!(DeleteMatching = {
    Ok(200),
    /// Subtask does not exist.
    SubtaskNotFound(404, error),
});

response!(SolveMatching = {
    Ok(201) => SolveMatchingFeedback,
    /// Try again later. `details` contains the number of seconds to wait.
    TooManyRequests(429, error) => u64,
    /// Subtask does not exist.
    SubtaskNotFound(404, error),
    /// The user has not unlocked this matching.
    NoAccess(403, error),
    /// The solution list does not contain the same number of entries as the left and right lists.
    SolutionDifferentLength(400, error),
});

async fn get_matching(
    db: &DatabaseTransaction,
    task_id: Uuid,
    subtask_id: Uuid,
) -> Result<Option<(challenges_matchings::Model, challenges_subtasks::Model)>, ErrorResponse> {
    Ok(
        match challenges_matchings::Entity::find_by_id(subtask_id)
            .find_also_related(challenges_subtasks::Entity)
            .filter(challenges_subtasks::Column::TaskId.eq(task_id))
            .one(db)
            .await?
        {
            Some((matching, Some(subtask))) => Some((matching, subtask)),
            _ => None,
        },
    )
}

fn check_matching(
    left: &[String],
    right: &[String],
    solution: &[u8],
) -> Result<(), InvalidMatchingError> {
    let n = left.len();
    if right.len() != n {
        return Err(InvalidMatchingError::LeftRightDifferentLength);
    }
    if solution.len() != n {
        return Err(InvalidMatchingError::SolutionDifferentLength);
    }
    if let Some(&x) = solution.iter().find(|&&x| x >= n as _) {
        return Err(InvalidMatchingError::InvalidIndex(x));
    }
    let mut not_matched: HashSet<u8> = (0..n as _).collect();
    for &x in solution {
        not_matched.remove(&x);
    }
    if !not_matched.is_empty() {
        return Err(InvalidMatchingError::RightEntriesNotMatched(not_matched));
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum InvalidMatchingError {
    LeftRightDifferentLength,
    SolutionDifferentLength,
    InvalidIndex(u8),
    RightEntriesNotMatched(HashSet<u8>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_matching() {
        let left = ["A".into(), "B".into(), "C".into()];
        let right = ["X".into(), "Y".into(), "Z".into()];
        let solution = [2, 0, 1];
        assert_eq!(check_matching(&left, &right, &solution), Ok(()));
        assert_eq!(
            check_matching(&left, &right, &[2, 0, 1, 3]),
            Err(InvalidMatchingError::SolutionDifferentLength)
        );
        assert_eq!(
            check_matching(&left, &right, &[2, 0, 3]),
            Err(InvalidMatchingError::InvalidIndex(3))
        );
        assert_eq!(
            check_matching(&left, &right, &[2, 0, 2]),
            Err(InvalidMatchingError::RightEntriesNotMatched([1].into()))
        );
        assert_eq!(
            check_matching(&left, &right, &[1, 1, 1]),
            Err(InvalidMatchingError::RightEntriesNotMatched([0, 2].into()))
        );
        assert_eq!(
            check_matching(&left, &["foo".into()], &solution),
            Err(InvalidMatchingError::LeftRightDifferentLength)
        );
    }
}