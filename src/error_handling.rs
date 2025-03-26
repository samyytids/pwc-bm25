use bm25::ScoredDocument;
use sqlx;
use std::sync::{PoisonError, TryLockError};
use tonic::{Code, Status};

#[derive(Debug)]
pub struct DbError {
    message: String,
}

#[derive(Debug)]
pub struct LockError {
    message: String,
}

impl DbError {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl LockError {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl From<sqlx::Error> for DbError {
    fn from(value: sqlx::Error) -> Self {
        Self::new(value.to_string())
    }
}

impl From<Box<dyn std::error::Error>> for DbError {
    fn from(value: Box<dyn std::error::Error>) -> Self {
        Self::new(value.to_string())
    }
}

impl<T> From<PoisonError<T>> for LockError {
    fn from(value: PoisonError<T>) -> Self {
        Self::new(value.to_string())
    }
}

impl<T> From<TryLockError<T>> for LockError {
    fn from(value: TryLockError<T>) -> Self {
        Self::new(value.to_string())
    }
}

impl From<DbError> for Status {
    fn from(value: DbError) -> Self {
        Status::new(Code::Internal, value.message)
    }
}

impl From<LockError> for Status {
    fn from(value: LockError) -> Self {
        Status::new(Code::Internal, value.message)
    }
}

pub fn check_len<T>(scores: &[ScoredDocument<T>], num_results: usize) -> Result<usize, Status> {
    let num_scores = scores.len();
    if num_results > 100_000 {
        return Err(Status::new(
            Code::Internal,
            format!("Num results > 10_000. gRPC doesn't like that."),
        ));
    } else if num_results > num_scores {
        Ok(num_scores)
    } else {
        Ok(num_results)
    }
    
}
