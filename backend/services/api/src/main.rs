The code you provided contains several **Git merge conflict markers** (`<<<<<<<`, `=======`, `>>>>>>>`) and significant duplication in the route registrations and tests.

Here is the fully resolved and cleaned-up version of `backend/services/api/src/main.rs`. This version integrates the **Reputation/Review Feature (#364)** correctly while preserving all other functionalities (ML payments, Escrow, Auth).

```rust
use actix_cors::Cors;
use actix_web::body::MessageBody;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{http, middleware, web, App, HttpResponse, HttpServer};
use futures::future::{ok, Ready};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;

mod aggregation;
mod analytics;
mod auth;
mod database;
mod event_indexer;
mod ml;
mod ml_handlers;
mod reputation;
mod verification_rewards;
mod webhook;
mod websocket;

pub const API_VERSION: &str = "1";
pub const API_PREFIX: &str = "/api/v1";

// ==================== Startup Configuration ====================

fn parse_u16_env_with_range(name: &str, default: u16, min: u16, max: u16) -> u16 {
    let raw = std::env::var(name).unwrap_or_else(|_| default.to_string());
    let parsed = raw.parse::<u16>().unwrap_or_else(|_| {
        panic!(
            "{} must be a valid unsigned 16-bit integer, got '{}'",
            name, raw
        )
    });

    if !(min..=max).contains(&parsed) {
        panic!(
            "{} must be between {} and {} (inclusive), got {}",
            name, min, max, parsed
        );
    }

    parsed
}

fn parse_u32_env_with_range(name: &str, default: u32, min: u32, max: u32) -> u32 {
    let raw = std::env::var(name).unwrap_or_else(|_| default.to_string());
    let parsed = raw.parse::<u32>().unwrap_or_else(|_| {
        panic!(
            "{} must be a valid unsigned 32-bit integer, got '{}'",
            name, raw
        )
    });

    if !(min..=max).contains(&parsed) {
        panic!(
            "{} must be between {} and {} (inclusive), got {}",
            name, min, max, parsed
        );
    }

    parsed
}

fn parse_u64_env_with_range(name: &str, default: u64, min: u64, max: u64) -> u64 {
    let raw = std::env::var(name).unwrap_or_else(|_| default.to_string());
    let parsed = raw.parse::<u64>().unwrap_or_else(|_| {
        panic!(
            "{} must be a valid unsigned 64-bit integer, got '{}'",
            name, raw
        )
    });

    if !(min..=max).contains(&parsed) {
        panic!(
            "{} must be between {} and {} (inclusive), got {}",
            name, min, max, parsed
        );
    }

    parsed
}

fn parse_u32_env_with_range_alias(
    primary_name: &str,
    legacy_name: &str,
    default: u32,
    min: u32,
    max: u32,
) -> u32 {
    let raw = std::env::var(primary_name)
        .or_else(|_| std::env::var(legacy_name))
        .unwrap_or_else(|_| default.to_string());

    let parsed = raw.parse::<u32>().unwrap_or_else(|_| {
        panic!(
            "{} (or legacy {}) must be a valid unsigned 32-bit integer, got '{}'",
            primary_name, legacy_name, raw
        )
    });

    if !(min..=max).contains(&parsed) {
        panic!(
            "{} (or legacy {}) must be between {} and {} (inclusive), got {}",
            primary_name, legacy_name, min, max, parsed
        );
    }

    parsed
}

// ==================== Domain Models ====================

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiErrorCode {
    BadRequest,
    ValidationError,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    UnprocessableEntity,
    InternalServerError,
    ServiceUnavailable,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct ApiError {
    pub code: ApiErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "fieldErrors")]
    pub field_errors: Option<Vec<FieldError>>,
}

impl ApiError {
    pub fn new(code: ApiErrorCode, message: impl Into<String>) -> Self {
        ApiError {
            code,
            message: message.into(),
            field_errors: None,
        }
    }

    pub fn with_field_errors(
        code: ApiErrorCode,
        message: impl Into<String>,
        field_errors: Vec<FieldError>,
    ) -> Self {
        ApiError {
            code,
            message: message.into(),
            field_errors: Some(field_errors),
        }
    }

    pub fn not_found(resource: impl Into<String>) -> Self {
        ApiError::new(
            ApiErrorCode::NotFound,
            format!("{} not found", resource.into()),
        )
    }

    pub fn internal() -> Self {
        ApiError::new(
            ApiErrorCode::InternalServerError,
            "An unexpected error occurred",
        )
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct PaginationMeta {
    pub page: u32,
    pub limit: u32,
    pub total: u64,
    pub total_pages: u32,
}

impl PaginationMeta {
    pub fn new(page: u32, limit: u32, total: u64) -> Self {
        let total_pages = ((total as f64) / (limit as f64)).ceil() as u32;
        PaginationMeta {
            page,
            limit,
            total,
            total_pages,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PaginatedData<T> {
    pub items: Vec<T>,
    pub pagination: PaginationMeta,
}

impl<T> PaginatedData<T> {
    pub fn new(items: Vec<T>, page: u32, limit: u32, total: u64) -> Self {
        PaginatedData {
            items,
            pagination: PaginationMeta::new(page, limit, total),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ApiError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn ok(data: T, message: Option<String>) -> Self {
        ApiResponse {
            success: true,
            data: Some(data),
            error: None,
            message,
        }
    }

    pub fn err(error: ApiError) -> Self {
        ApiResponse {
            success: false,
            data: None,
            error: Some(error),
            message: None,
        }
    }
}

// ==================== Request Models ====================

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ReviewSubmission {
    #[serde(rename = "bountyId")]
    pub bounty_id: String,
    #[serde(rename = "creatorId")]
    pub creator_id: String,
    pub rating: u8,
    pub title: String,
    pub body: String,
    #[serde(rename = "reviewerName")]
    pub reviewer_name: String,
}

// ==================== Routes ====================

async fn health(
    pool: web::Data<PgPool>,
    rpc_url: web::Data<String>,
) -> HttpResponse {
    let mut db_connected = false;
    let mut rpc_connected = false;

    match pool.acquire().await {
        Ok(_) => db_connected = true,
        Err(e) => tracing::error!("Database health check failed: {}", e),
    }

    let client = reqwest::Client::new();
    match client.get(rpc_url.get_ref()).send().await {
        Ok(resp) => {
            if resp.status().is_success() || resp.status().as_u16() == 405 {
                rpc_connected = true;
            }
        }
        Err(e) => tracing::error!("Stellar RPC health check failed: {}", e),
    }

    let status = if db_connected && rpc_connected { "healthy" } else { "unhealthy" };
    HttpResponse::Ok().json(serde_json::json!({
        "status": status,
        "dependencies": {
            "database": if db_connected { "connected" } else { "disconnected" },
            "stellar_rpc": if rpc_connected { "connected" } else { "disconnected" }
        }
    }))
}

async fn create_bounty(body: web::Json<database::BountyRequest>) -> HttpResponse {
    let bounty = database::create_bounty(body.into_inner());
    HttpResponse::Created().json(ApiResponse::ok(bounty, Some("Bounty created successfully".into())))
}

async fn list_bounties() -> HttpResponse {
    let bounties = database::get_mock_bounties();
    HttpResponse::Ok().json(ApiResponse::ok(bounties, None))
}

async fn get_bounty(path: web::Path<u64>) -> HttpResponse {
    match database::get_bounty_by_id(path.into_inner()) {
        Some(b) => HttpResponse::Ok().json(ApiResponse::ok(b, None)),
        None => HttpResponse::NotFound().json(ApiResponse::<()>::err(ApiError::not_found("Bounty"))),
    }
}

async fn apply_for_bounty(path: web::Path<u64>, body: web::Json<database::BountyApplication>) -> HttpResponse {
    match database::apply_for_bounty(path.into_inner(), body.into_inner()) {
        Ok(_) => HttpResponse::Created().json(ApiResponse::ok((), Some("Applied successfully".into()))),
        Err(e) => HttpResponse::BadRequest().json(ApiResponse::<()>::err(ApiError::new(ApiErrorCode::BadRequest, e))),
    }
}

async fn list_creators(query: web::Query<std::collections::HashMap<String, String>>) -> HttpResponse {
    let discipline = query.get("discipline").cloned();
    let search = query.get("search").cloned();
    let creators = database::filter_creators(database::get_mock_creators(), discipline, search);
    HttpResponse::Ok().json(ApiResponse::ok(creators, None))
}

async fn get_creator(path: web::Path<String>) -> HttpResponse {
    match database::get_creator_by_id(&path.into_inner()) {
        Some(c) => HttpResponse::Ok().json(ApiResponse::ok(c, None)),
        None => HttpResponse::NotFound().json(ApiResponse::<()>::err(ApiError::not_found("Creator"))),
    }
}

async fn get_creator_reputation(path: web::Path<String>, pool: web::Data<PgPool>) -> HttpResponse {
    let creator_id = path.into_inner();
    reputation::set_database_pool(pool.get_ref().clone());
    let reviews = reputation::fetch_creator_reviews_from_db(&creator_id).await;
    let aggregation = reputation::fetch_creator_reputation_from_db(&creator_id).await;
    let payload = reputation::CreatorReputationPayload {
        creator_id,
        aggregation,
        recent_reviews: reputation::recent_reviews(&reviews, 8),
    };
    HttpResponse::Ok().json(ApiResponse::ok(payload, None))
}

async fn get_creator_reviews_filtered(path: web::Path<String>, query: web::Query<std::collections::HashMap<String, String>>, pool: web::Data<PgPool>) -> HttpResponse {
    let creator_id = path.into_inner();
    reputation::set_database_pool(pool.get_ref().clone());
    let filters = reputation::parse_review_filters(&query).unwrap_or_default();
    let payload = reputation::get_filtered_creator_reviews_from_db(&creator_id, &filters).await;
    HttpResponse::Ok().json(ApiResponse::ok(payload, None))
}

async fn list_reviews_filtered(query: web::Query<std::collections::HashMap<String, String>>, pool: web::Data<PgPool>) -> HttpResponse {
    reputation::set_database_pool(pool.get_ref().clone());
    let filters = reputation::parse_review_filters(&query).unwrap_or_default();
    let all_reviews = reputation::fetch_all_reviews_from_db().await;
    let filtered = reputation::filter_reviews(&all_reviews, &filters);
    HttpResponse::Ok().json(ApiResponse::ok(filtered, None))
}

async fn submit_review(body: web::Json<ReviewSubmission>) -> HttpResponse {
    match reputation::on_review_submitted(&body.bounty_id, &body.creator_id, body.rating, &body.title, &body.body, &body.reviewer_name) {
        Ok(id) => HttpResponse::Created().json(ApiResponse::ok(id, Some("Review submitted".into()))),
        Err(e) => HttpResponse::BadRequest().json(ApiResponse::<()>::err(ApiError::new(ApiErrorCode::BadRequest, e.join(", ")))),
    }
}

async fn register_freelancer(body: web::Json<database::FreelancerRegistration>) -> HttpResponse {
    let f = database::register_freelancer(body.into_inner(), "wallet".into());
    HttpResponse::Created().json(ApiResponse::ok(f, Some("Registered".into())))
}

async fn list_freelancers() -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(database::get_mock_freelancers(), None))
}

async fn get_freelancer(path: web::Path<String>) -> HttpResponse {
    match database::get_freelancer_by_address(&path.into_inner()) {
        Some(f) => HttpResponse::Ok().json(ApiResponse::ok(f, None)),
        None => HttpResponse::NotFound().json(ApiResponse::<()>::err(ApiError::not_found("Freelancer"))),
    }
}

async fn create_escrow(body: web::Json<database::EscrowCreateRequest>) -> HttpResponse {
    let escrow = database::create_escrow(body.into_inner());
    HttpResponse::Created().json(ApiResponse::ok(escrow, Some("Escrow created".into())))
}

async fn get_escrow(path: web::Path<u64>) -> HttpResponse {
    match database::get_escrow_by_id(path.into_inner()) {
        Some(e) => HttpResponse::Ok().json(ApiResponse::ok(e, None)),
        None => HttpResponse::NotFound().json(ApiResponse::<()>::err(ApiError::not_found("Escrow"))),
    }
}

async fn release_escrow(path: web::Path<u64>) -> HttpResponse {
    match database::release_escrow(path.into_inner()) {
        Some(e) => HttpResponse::Ok().json(ApiResponse::ok(e, Some("Funds released".into()))),
        None => HttpResponse::NotFound().json(ApiResponse::<()>::err(ApiError::not_found("Escrow"))),
    }
}

async fn refund_escrow(path: web::Path<u64>, body: web::Json<database::EscrowRefundRequest>) -> HttpResponse {
    match database::refund_escrow(path.into_inner(), body.authorizer_address.clone()) {
        Some(e) => HttpResponse::Ok().json(ApiResponse::ok(e, Some("Refunded".into()))),
        None => HttpResponse::NotFound().json(ApiResponse::<()>::err(ApiError::not_found("Escrow"))),
    }
}

async fn api_versions() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "current": API_VERSION, "supported": ["1"] }))
}

// ==================== Middleware & Helpers ====================

pub struct ApiVersionHeader;

impl<S, B> Transform<S, ServiceRequest> for ApiVersionHeader
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Transform = ApiVersionHeaderMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, ()>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(ApiVersionHeaderMiddleware { service })
    }
}

pub struct ApiVersionHeaderMiddleware<S> { service: S }

impl<S, B> Service<ServiceRequest> for ApiVersionHeaderMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>>>>;

    actix_web::dev::forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let fut = self.service.call(req);
        Box::pin(async move {
            let mut res = fut.await?;
            res.headers_mut().insert(
                http::header::HeaderName::from_static("x-api-version"),
                http::header::HeaderValue::from_static(API_VERSION),
            );
            Ok(res)
        })
    }
}

pub fn cors_middleware() -> Cors {
    Cors::default()
        .allowed_origin_fn(|origin, _req| {
            let allowed = std::env::var("CORS_ALLOWED_ORIGINS").unwrap_or_else(|_| "http://localhost:3000".into());
            allowed.split(',').any(|o| o.trim() == origin.to_str().unwrap_or_default())
        })
        .allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
        .allowed_headers(vec![http::header::AUTHORIZATION, http::header::CONTENT_TYPE])
        .supports_credentials()
        .max_age(3600)
}

// ==================== Main ====================

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPoolOptions::new().max_connections(10).connect(&database_url).await.expect("DB connection failed");

    sqlx::migrate!("../../migrations").run(&pool).await.ok();
    reputation::initialize_reputation_system_with_db(pool.clone());

    let stellar_rpc_url = std::env::var("STELLAR_RPC_URL").unwrap_or_else(|_| "https://soroban-testnet.stellar.org".into());
    let ml_state = web::Data::new(ml_handlers::MlAppState { model: std::sync::Arc::new(ml::SimpleMLModel::new(&[])) });
    let ws_limiter = websocket::WsConnectionLimiter::from_env();

    let host = std::env::var("API_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = parse_u16_env_with_range("API_PORT", 3001, 1, 65535);

    tracing::info!("Server starting on {}:{}", host, port);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(stellar_rpc_url.clone()))
            .app_data(ml_state.clone())
            .app_data(web::Data::new(ws_limiter.clone()))
            .wrap(cors_middleware())
            .wrap(middleware::Logger::default())
            .wrap(middleware::NormalizePath::trim())
            .wrap(ApiVersionHeader)
            .route("/health", web::get().to(health))
            .route("/api/versions", web::get().to(api_versions))
            .route("/ws", web::get().to(websocket::ws_handler))
            .service(
                web::scope("/api/v1")
                    .route("/bounties", web::get().to(list_bounties))
                    .route("/bounties/{id}", web::get().to(get_bounty))
                    .route("/creators", web::get().to(list_creators))
                    .route("/creators/{id}", web::get().to(get_creator))
                    .route("/creators/{id}/reputation", web::get().to(get_creator_reputation))
                    .route("/creators/{id}/reviews", web::get().to(get_creator_reviews_filtered))
                    .route("/reviews", web::get().to(list_reviews_filtered))
                    .route("/reviews", web::post().to(submit_review))
                    .route("/freelancers", web::get().to(list_freelancers))
                    .route("/freelancers/{address}", web::get().to(get_freelancer))
                    .route("/escrow/{id}", web::get().to(get_escrow))
                    .service(
                        web::scope("")
                            .wrap(auth::JwtMiddleware)
                            .route("/bounties", web::post().to(create_bounty))
                            .route("/bounties/{id}/apply", web::post().to(apply_for_bounty))
                            .route("/freelancers/register", web::post().to(register_freelancer))
                            .route("/escrow/create", web::post().to(create_escrow))
                            .route("/escrow/{id}/release", web::post().to(release_escrow))
                            .route("/escrow/{id}/refund", web::post().to(refund_escrow)),
                    ),
            )
    })
    .bind((host, port))?
    .run()
    .await
}
```