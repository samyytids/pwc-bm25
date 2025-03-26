use sqlx::postgres::{PgPool, PgRow, Postgres};
use sqlx::{FromRow, Pool};
use std::env;
pub mod score {
    tonic::include_proto!("score");
}

pub enum QueryType {
    DatasetQuery,
    PaperQuery,
}

#[derive(FromRow)]
pub struct Paper {
    pub paper_id: Vec<u8>,
    pub dataset_id: i32,
    #[sqlx(rename = "abstract")]
    pub abstract_text: String,
}

#[derive(FromRow)]
pub struct Dataset {
    pub dataset_id: i32,
    pub source_description: String,
}

pub async fn connect() -> Result<Pool<Postgres>, Box<dyn std::error::Error>> {
    let user = env::var("POSTGRES_USER").unwrap();
    let password = env::var("POSTGRES_PASSWORD").unwrap();
    let db = env::var("POSTGRES_DB").unwrap();
    let host = env::var("POSTGRES_HOST").unwrap();
    let port = env::var("POSTGRES_PORT").unwrap();
    let database_url = format!(
        "postgresql://{}:{}@{}:{}/{}",
        user, password, host, port, db
    );
    let pool = PgPool::connect(&database_url).await?;
    Ok(pool)
}

pub async fn get_data<T>(
    query_type: QueryType,
    pool: &Pool<Postgres>,
) -> Result<Vec<T>, Box<dyn std::error::Error>>
where
    T: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    let rows = match query_type {
        QueryType::DatasetQuery => {
            let rows = sqlx::query_as::<_, T>("SELECT dataset_id, source_description FROM dataset WHERE source_description IS NOT NULL").fetch_all(pool).await?;
            rows
        }
        QueryType::PaperQuery => {
            let rows = sqlx::query_as::<_, T>("SELECT dp.paper_id, dp.dataset_id, abstract FROM dataset_paper dp JOIN paper p ON dp.paper_id = p.paper_id WHERE abstract IS NOT NULL").fetch_all(pool).await?;
            rows
        }
    };

    Ok(rows)
}
