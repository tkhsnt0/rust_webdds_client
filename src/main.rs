use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, put},
    Json, Router
};
use rustdds::with_key::Sample;
use rustdds::*;
use serde::{Deserialize, Serialize};
use sled;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use utoipa::OpenApi;
use with_key::DataWriter;
use utoipa::ToSchema;
use utoipa_swagger_ui::SwaggerUi;

type DataWriterState = Arc<Mutex<DataWriter<SensorConfig>>>;

#[derive(Serialize, Deserialize, Clone, Debug, Default, ToSchema)]
struct SensorList {
    name: String,    
    path: String,    
}
#[derive(Serialize, Deserialize, Clone, Debug, Default, ToSchema)]
struct SensorConfig {
    sensor_type: String,
    frequency: u32,
    power: u32,
    squelti: u32,
}
impl Keyed for SensorConfig {
    type K = String;
    fn key(&self) -> Self::K {
        self.sensor_type.clone()
    }
}
#[derive(Serialize, Deserialize, Clone, Debug, Default, ToSchema)]
struct SensorStatus {
    sensor_type: String,
    frequency: u32,
    power: u32,
    squelti: u32,
}
impl Keyed for SensorStatus {
    type K = String;
    fn key(&self) -> Self::K {
        self.sensor_type.clone()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // dds
    let domain_participant = DomainParticipantBuilder::new(0).build()?;
    let qos = QosPolicyBuilder::new().build();
    let topic_sensor_config = domain_participant
        .create_topic(
            "SensorConfig".to_string(),
            "Topic: SensorConfig".to_string(),
            &qos,
            TopicKind::WithKey,
        )
        .unwrap();
    let topic_name = "SensorStatus".to_string();
    let topic_sensor_status = domain_participant
        .create_topic(
            topic_name.clone(),
            "Topic: SensorStatus".to_string(),
            &qos,
            TopicKind::WithKey,
        )
        .unwrap();
    let publisher = domain_participant.create_publisher(&qos).unwrap();
    let writer = publisher
        .create_datawriter_cdr::<SensorConfig>(&topic_sensor_config, Some(qos.clone()))
        .unwrap();
    let subscriber = domain_participant.create_subscriber(&qos).unwrap();
    let reader = subscriber
        .create_datareader_cdr::<SensorStatus>(&topic_sensor_status, Some(qos.clone()))
        .unwrap();
    let mut async_reader = reader.async_sample_stream();

    // db
    let db = sled::open(topic_name).unwrap();
    let db_status = db.open_tree("status");
    let db_clone_for_sub = db_status.clone().unwrap();
    let db_clone_for_axum = db_status.clone().unwrap();

    // background subscriber
    tokio::spawn(async move {
        println!("subscriber start");
        while let Some(Ok(Sample::Value(v))) = &mut async_reader.next().await {
            if let Ok(json) = serde_json::to_string(&v) {
                db_clone_for_sub.insert(v.key(), json.as_bytes()).unwrap();
                println!("subscribe: {:?}", v);
            }
        }
    });

    // state
    let writer_for_axum = Arc::new(Mutex::new(writer));

    // build our application with a route
    let mut doc = ApiDoc::openapi();
    doc.info.title = String::from("OpenAPI Documents");    
    let app = Router::new()
        .route("/sensor/list", get(get_handler_sensor_list))
        .route("/sensor/config", put(put_handler_sensor_config))
        .with_state(writer_for_axum)
        .route("/sensor/status", get(get_handler_sensor_status))
        .with_state(db_clone_for_axum)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-doc/openapi.json", doc));

    let listener = tokio::net::TcpListener::bind("localhost:3000")
        .await
        .unwrap();
    axum::serve(listener, app.into_make_service()).await.unwrap();

    Ok(())
}


#[utoipa::path(
    get,
    path = "/sensor/list",
    responses(
        (status = 200, body = [SensorList], description = "Get sensors name"),        
    ),
    tag = "get_handler_sensor_list"
)]
async fn get_handler_sensor_list() -> (StatusCode, Json<Vec<SensorList>>) {
    let api_list = Json(vec![
        SensorList{
            name: String::from("Sensor_Module_A"),
            path: String::from("/sensor/module_A"),
        },
        SensorList{
            name: String::from("Sensor_Module_B"),
            path: String::from("/sensor/module_B"),
        },
    ]);
    (StatusCode::OK, api_list)
}

#[utoipa::path(
    put,
    path = "/sensor/config",
    responses(
        (status = 200, body = [SensorConfig], description = "Set config to sensor"),
        (status = 500, body = [SensorConfig], description = "Internal server error")
    ),
    tag = "put_handler_sensor_config"
)]
async fn put_handler_sensor_config(
    State(writer): State<DataWriterState>,
    Json(payload): Json<SensorConfig>,
) -> (StatusCode, Json<SensorConfig>) {
    let writer = &mut writer.lock().await;
    if let Ok(_) = writer.async_write(payload.clone(), None).await {
        (StatusCode::OK, Json(payload))
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SensorConfig {
                ..Default::default()
            }),
        )
    }
}
#[utoipa::path(
    get,
    path = "/sensor/status",
    responses(
        (status = 200, body = [SensorStatus], description = "Get status from sensor"),
        (status = 500, body = [SensorStatus], description = "Internal server error")
    ),
    tag = "get_handler_sensor_status",
)]
async fn get_handler_sensor_status(
    State(db_tree): State<sled::Tree>,
) -> (StatusCode, Json<SensorConfig>) {
    if let Ok(Some(value)) = db_tree.get("radio") {
        let deser_json = serde_json::from_slice(&value).unwrap();
        (StatusCode::OK, Json(deser_json))
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SensorConfig {
                ..Default::default()
            }),
        )
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_handler_sensor_list,
        put_handler_sensor_config,
        get_handler_sensor_status,        
    ),
    components(schemas(
        SensorList,
        SensorConfig,
        SensorStatus,        
    )),
    tags((name = "Rust_WebDDS_Client", description="This is Sample Axum with DDS pub/sub"))
)]
struct ApiDoc;
