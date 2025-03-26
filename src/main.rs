mod error_handling;
mod sql;
use error_handling::{DbError, LockError, check_len};
use sql::{Dataset, Paper, QueryType, connect, get_data};
use sqlx::{Pool, Postgres};
use tokio;
pub mod score {
    tonic::include_proto!("score");
}

use bm25::{Embedder, EmbedderBuilder, Language, Scorer};
use score::score_getter_server::{ScoreGetter, ScoreGetterServer};
use score::{
    DatasetScoreResponse, Empty, PaperScoreResponse, PopulateRequest, PopulateType, ScoreRequest,
};

use std::sync::Mutex;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

#[derive(Hash, Eq, PartialEq, Clone)]
struct PaperId {
    paper_id: Vec<u8>,
    dataset_id: i32,
}

struct Bm25Model<T> {
    scorer: Mutex<Scorer<T>>,
    embedder: Mutex<Embedder>,
}

struct ScoreGetterService {
    dataset_model: Bm25Model<i32>,
    paper_model: Bm25Model<PaperId>,
    pool: Pool<Postgres>,
}

#[tonic::async_trait]
impl ScoreGetter for ScoreGetterService {
    type PaperScoreStream = ReceiverStream<Result<PaperScoreResponse, Status>>;
    type DatasetScoreStream = ReceiverStream<Result<DatasetScoreResponse, Status>>;

    async fn dataset_score(
        &self,
        request: Request<ScoreRequest>,
    ) -> Result<Response<Self::DatasetScoreStream>, Status> {
        let (tx, rx) = mpsc::channel(4);
        let model = &self.dataset_model;
        let query = request.get_ref().query.clone();
        let num_results = request.get_ref().num_results as usize;
        let embedder = model.embedder.lock().map_err(|e| LockError::from(e))?;
        let embedded_query = embedder.embed(&query);
        let scores = model
            .scorer
            .lock()
            .map_err(|e| LockError::from(e))?
            .matches(&embedded_query);

        let new_len = check_len(&scores, num_results)?;
        tokio::spawn(async move {
            for score in &scores[..new_len] {
                tx.send(Ok(DatasetScoreResponse {
                    dataset_id: score.id,
                    score: score.score,
                }))
                .await
                .unwrap();
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
    async fn paper_score(
        &self,
        request: Request<ScoreRequest>,
    ) -> Result<Response<Self::PaperScoreStream>, Status> {
        let (tx, rx) = mpsc::channel(4);
        let model = &self.paper_model;
        let query = request.get_ref().query.clone();
        let num_results = request.get_ref().num_results as usize;
        let embedder = model.embedder.lock().map_err(|e| LockError::from(e))?;
        let embedded_query = embedder.embed(&query);
        let scores = model
            .scorer
            .lock()
            .map_err(|e| LockError::from(e))?
            .matches(&embedded_query);

        let new_len = check_len(&scores, num_results)?;
        tokio::spawn(async move {
            for score in &scores[..new_len] {
                tx.send(Ok(PaperScoreResponse {
                    paper_id: score.id.paper_id.clone(),
                    dataset_id: score.id.dataset_id,
                    score: score.score,
                }))
                .await
                .unwrap();
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
    async fn populate(&self, request: Request<PopulateRequest>) -> Result<Response<Empty>, Status> {
        let populate_type = request.get_ref().populate_type();
        let result = match populate_type {
            PopulateType::Paper => {
                let data = get_data::<Paper>(QueryType::PaperQuery, &self.pool)
                    .await
                    .map_err(|e| DbError::from(e))?;

                let paper_corpus: Vec<&str> =
                    data.iter().map(|s| s.abstract_text.as_str()).collect();

                let paper_embedder: Embedder =
                    EmbedderBuilder::with_fit_to_corpus(Language::English, &paper_corpus).build();

                let mut scorer = self
                    .paper_model
                    .scorer
                    .lock()
                    .map_err(|e| LockError::from(e))?;

                for (i, document) in paper_corpus.iter().enumerate() {
                    let document_embedding = paper_embedder.embed(document);
                    scorer.upsert(
                        &PaperId {
                            paper_id: data[i].paper_id.clone(),
                            dataset_id: data[i].dataset_id,
                        },
                        document_embedding,
                    );
                }

                let mut embedder = self
                    .paper_model
                    .embedder
                    .lock()
                    .map_err(|e| LockError::from(e))?;

                *embedder = paper_embedder;
                Ok(Response::new(Empty {}))
            }
            PopulateType::Dataset => {
                let data = get_data::<Dataset>(QueryType::DatasetQuery, &self.pool)
                    .await
                    .map_err(|e| DbError::from(e))?;

                let dataset_corpus: Vec<&str> =
                    data.iter().map(|s| s.source_description.as_str()).collect();

                let dataset_embedder: Embedder =
                    EmbedderBuilder::with_fit_to_corpus(Language::English, &dataset_corpus).build();

                let mut scorer = self
                    .dataset_model
                    .scorer
                    .lock()
                    .map_err(|e| LockError::from(e))?;

                for (i, document) in dataset_corpus.iter().enumerate() {
                    let document_embedding = dataset_embedder.embed(document);
                    scorer.upsert(&data[i].dataset_id, document_embedding);
                    
                }
                
                let mut embedder = self
                    .dataset_model
                    .embedder
                    .lock()
                    .map_err(|e| LockError::from(e))?;

                *embedder = dataset_embedder;
                Ok(Response::new(Empty {}))
            }
        };
        result
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:10000".parse().unwrap();
    let pool = connect().await?;
    let paper_data: Vec<Paper> = vec![];
    let dataset_data: Vec<Dataset> = vec![];

    let paper_corpus: Vec<&str> = paper_data
        .iter()
        .map(|s| s.abstract_text.as_str())
        .collect();
    let dataset_corpus: Vec<&str> = dataset_data
        .iter()
        .map(|s| s.source_description.as_str())
        .collect();
    println!("Embedding papers");
    let paper_embedder: Embedder =
        EmbedderBuilder::with_fit_to_corpus(Language::English, &paper_corpus).build();
    println!("Embedding datasets");
    let dataset_embedder: Embedder =
        EmbedderBuilder::with_fit_to_corpus(Language::English, &dataset_corpus).build();

    let mut paper_scorer = Scorer::<PaperId>::new();
    let mut dataset_scorer = Scorer::<i32>::new();
    println!("Training papers");
    for (i, document) in paper_corpus.iter().enumerate() {
        let document_embedding = paper_embedder.embed(document);
        paper_scorer.upsert(
            &PaperId {
                paper_id: paper_data[i].paper_id.clone(),
                dataset_id: paper_data[i].dataset_id,
            },
            document_embedding,
        );
    }
    println!("Training datasets");
    for (i, document) in dataset_corpus.iter().enumerate() {
        let document_embedding = dataset_embedder.embed(document);
        dataset_scorer.upsert(&dataset_data[i].dataset_id, document_embedding);
    }
    let scorer = ScoreGetterService {
        paper_model: Bm25Model {
            scorer: Mutex::new(paper_scorer),
            embedder: Mutex::new(paper_embedder),
        },
        dataset_model: Bm25Model {
            scorer: Mutex::new(dataset_scorer),
            embedder: Mutex::new(dataset_embedder),
        },
        pool,
    };
    let score_svc = ScoreGetterServer::new(scorer);
    println!("Ready!");
    Server::builder().add_service(score_svc).serve(addr).await?;

    Ok(())
}
