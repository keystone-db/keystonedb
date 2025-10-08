/// Notebook HTTP server implementation

use anyhow::Result;
use axum::{
    extract::{Path as AxumPath, State, WebSocketUpgrade},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post, delete},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use kstone_api::Database;

use super::{
    handlers,
    websocket,
    assets,
    NotebookConfig,
};

/// The notebook server
pub struct NotebookServer {
    db: Arc<Database>,
    config: NotebookConfig,
}

impl NotebookServer {
    /// Create a new notebook server
    pub fn new(db: Arc<Database>, config: NotebookConfig) -> Self {
        Self { db, config }
    }

    /// Start serving the notebook interface
    pub async fn serve(self) -> Result<()> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let app = self.create_app();

        let listener = tokio::net::TcpListener::bind(&addr).await?;

        tracing::info!("Notebook server listening on {}", addr);

        axum::serve(listener, app).await?;

        Ok(())
    }

    /// Create the Axum application with all routes
    fn create_app(self) -> Router {
        let state = AppState {
            db: self.db.clone(),
            config: self.config.clone(),
        };

        Router::new()
            // Static assets (HTML, CSS, JS)
            .route("/", get(serve_index))
            .route("/assets/*path", get(serve_asset))

            // Notebook API endpoints
            .route("/api/notebooks", get(handlers::list_notebooks))
            .route("/api/notebooks", post(handlers::create_notebook))
            .route("/api/notebooks/:id", get(handlers::get_notebook))
            .route("/api/notebooks/:id", post(handlers::update_notebook))
            .route("/api/notebooks/:id", delete(handlers::delete_notebook))

            // Cell operations
            .route("/api/notebooks/:id/cells", post(handlers::add_cell))
            .route("/api/notebooks/:id/cells/:cell_id", post(handlers::update_cell))
            .route("/api/notebooks/:id/cells/:cell_id", delete(handlers::delete_cell))
            .route("/api/notebooks/:id/cells/:cell_id/execute", post(handlers::execute_cell))

            // Database info
            .route("/api/database/info", get(handlers::database_info))
            .route("/api/database/schema", get(handlers::database_schema))

            // WebSocket for real-time query execution
            .route("/ws", get(websocket_handler))

            // Add CORS support for development
            .layer(CorsLayer::permissive())

            .with_state(state)
    }
}

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub config: NotebookConfig,
}

/// Serve the main index.html file
async fn serve_index() -> impl IntoResponse {
    match assets::get_asset("index.html") {
        Some(content) => Html(content).into_response(),
        None => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

/// Serve static assets
async fn serve_asset(AxumPath(path): AxumPath<String>) -> impl IntoResponse {
    match assets::get_asset(&path) {
        Some(content) => {
            // Determine content type based on file extension
            let content_type = match path.rsplit('.').next() {
                Some("js") => "application/javascript",
                Some("css") => "text/css",
                Some("html") => "text/html",
                Some("json") => "application/json",
                Some("png") => "image/png",
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("svg") => "image/svg+xml",
                _ => "application/octet-stream",
            };

            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", content_type)
                .body(axum::body::Body::from(content))
                .unwrap()
        }
        None => {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(axum::body::Body::from("Not found"))
                .unwrap()
        }
    }
}

/// WebSocket upgrade handler
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| websocket::handle_socket(socket, state))
}