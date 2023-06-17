use crossbeam::sync::WaitGroup;
use futures::{lock::Mutex, TryStreamExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio_context::context::Context;

use mongodb::{
    bson::{doc, oid::ObjectId, Bson},
    change_stream::{self, event::ChangeStreamEvent},
    options::{ChangeStreamOptions, FindOptions, FullDocumentType, IndexOptions, UpdateOptions},
    Client, IndexModel,
};

use crate::{Plugin, ServiceContent, Synchronize};

#[derive(Serialize, Deserialize, Clone, Debug)]
struct MongoContent {
    #[serde(rename(serialize = "_id", deserialize = "_id"))]
    id: String,

    #[serde(flatten)]
    content: ServiceContent,
}

impl PartialEq for MongoContent {
    fn eq(&self, other: &Self) -> bool {
        self.id.ne(&other.id)
    }
}

static SCHEMA_NAME: &str = "crossgate";
static COLLECTION_NAME: &str = "discovery";

#[derive(Debug, Clone)]
pub struct MongodbPlugin {
    inner: Arc<Mutex<Vec<MongoContent>>>,

    cache: Arc<Mutex<HashMap<String, Vec<MongoContent>>>>,

    schema: String,
    collection: String,

    client: Client,
}

impl MongodbPlugin {
    pub(crate) async fn new() -> Self {
        dotenv::dotenv().ok();
        let uri = std::env::var("REGISTER_ADDR").expect("REGISTER_ADDR is not set");

        let client = match mongodb::options::ClientOptions::parse_with_resolver_config(
            &uri,
            mongodb::options::ResolverConfig::cloudflare(),
        )
        .await
        {
            Ok(options) => Client::with_options(options).unwrap(),
            Err(e) => panic!("{:?}", e),
        };

        let mut s = Self {
            inner: Arc::new(Mutex::new(vec![])),
            cache: Arc::new(Mutex::new(HashMap::new())),

            schema: SCHEMA_NAME.to_string(),
            collection: COLLECTION_NAME.to_string(),

            client,
        };

        s.init().await;

        s
    }

    #[inline]
    async fn init(&mut self) {
        let _ = self
            .group_collection()
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "time":1, })
                    .options(
                        IndexOptions::builder()
                            .expire_after(std::time::Duration::from_secs(2))
                            .build(),
                    )
                    .build(),
                None,
            )
            .await;
    }

    #[inline]
    fn group_collection(&self) -> mongodb::Collection<MongoContent> {
        self.client
            .database(&self.schema)
            .collection(&self.collection)
    }

    #[inline]
    async fn update_cache(&mut self, key: String, c: &MongoContent) {
        let mut cache = self.cache.lock().await;
        if !cache.contains_key(&key) {
            cache.insert(key, vec![c.clone()]);
            return;
        }

        if let Some(v) = cache.get_mut(&key) {
            if !v.iter().any(|mc: &MongoContent| mc.ne(c)) {
                v.push(c.clone());
            }
        }
    }

    #[inline]
    async fn remove_cache(&mut self, id: &str) {
        let mut cache = self.cache.lock().await;
        for (_, values) in cache.iter_mut() {
            values.retain(|content| content.id != id);
        }
    }

    #[inline]
    async fn service_content_renewal(&mut self) {
        let contents = self.inner.lock().await;
        for c in contents.clone().iter() {
            let id = c.id.clone();
            if let Err(e) = self.service_content_apply(&id, &c.content).await {
                log::error!("{:?}", e);
            }
        }
    }

    async fn service_content_apply(
        &self,
        id: &str,
        content: &ServiceContent,
    ) -> anyhow::Result<()> {
        if self
            .group_collection()
            .count_documents(doc! {"_id":id}, None)
            .await?
            == 0
        {
            let _ = self
                .group_collection()
                .insert_one(
                    MongoContent {
                        id: id.clone().to_string(),
                        content: content.clone(),
                    },
                    None,
                )
                .await
                .map_err(|e| crate::PluginError::Error(e.to_string()))?;
        } else {
            self.group_collection()
                .update_one(
                    doc! {
                        "_id":id,
                    },
                    doc! {
                        "$set":
                        {
                            "time": mongodb::bson::DateTime::now(),
                        },
                    },
                    UpdateOptions::builder().upsert(false).build(),
                )
                .await
                .map_err(|e| crate::PluginError::Error(e.to_string()))?;
        }

        Ok(())
    }

    async fn list_mongo_content(
        &self,
        key: String,
        r#type: i32,
    ) -> anyhow::Result<Vec<MongoContent>> {
        let mut mongo_contents: Vec<MongoContent> = vec![];

        let mut cursor = self
            .group_collection()
            .find(
                doc! { "service": key.to_string(),"type": r#type },
                FindOptions::builder().sort(doc! { "_id": -1 }).build(),
            )
            .await
            .map_err(|e| crate::PluginError::Error(e.to_string()))?;

        while let Some(doc) = cursor
            .try_next()
            .await
            .map_err(|e| crate::PluginError::Error(e.to_string()))?
        {
            let key = if doc.content.service.eq("") {
                doc.id.clone()
            } else {
                doc.content.service.clone()
            };

            //init cache
            self.cache.lock().await.insert(key, vec![doc.clone()]);

            mongo_contents.push(doc);
        }

        Ok(mongo_contents)
    }

    async fn list_service_content(
        &self,
        key: &str,
        r#type: i32,
    ) -> anyhow::Result<Vec<ServiceContent>> {
        let mongo_contents = self.list_mongo_content(key.to_string(), r#type).await?;

        Ok(mongo_contents
            .iter()
            .map(|mc| mc.content.clone())
            .collect::<Vec<ServiceContent>>())
    }

    async fn mongo_content_builder(&self, content: &ServiceContent) -> String {
        let id = ObjectId::new().to_string();

        self.inner.lock().await.push(MongoContent {
            id: id.clone(),
            content: content.clone(),
        });

        id
    }

    async fn service_unset(&mut self) {
        let contents = self.inner.lock().await;
        for c in contents.iter() {
            self.group_collection()
                .delete_one(doc! {"_id":c.id.clone()}, None)
                .await
                .map_err(|e| log::error!("unset service {:?}", e))
                .unwrap();
        }
    }
}

#[crate::async_trait]
impl Plugin for MongodbPlugin {
    async fn register_service(&self, _: &str, val: ServiceContent) -> anyhow::Result<()> {
        self.service_content_apply(&self.mongo_content_builder(&val).await, &val)
            .await
    }

    async fn get_web_service(&self, k: &str) -> anyhow::Result<Vec<ServiceContent>> {
        if let Some(v) = self.cache.lock().await.get(k) {
            return Ok(v
                .iter()
                .map(|item| item.content.clone())
                .collect::<Vec<ServiceContent>>());
        }
        self.list_service_content(k, 1).await
    }

    async fn get_backend_service(&self, k: &str) -> anyhow::Result<(String, Vec<String>)> {
        let mut self_id: String = "".into();
        let inner = self.inner.lock().await;
        if let Some(v) = inner.iter().find(|c| c.content.service.eq(k)) {
            self_id = v.id.clone();
        }

        if let Some(v) = self.cache.lock().await.get(k) {
            return Ok((self_id, v.iter().map(|item| item.id.clone()).collect()));
        }

        let mut results = self
            .list_mongo_content(k.to_string(), 2)
            .await?
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<String>>();

        results.sort();

        Ok((self_id, results))
    }
}

#[crate::async_trait]
impl Synchronize for MongodbPlugin {
    async fn cache_refresh(&mut self) {
        let mut s = self.clone();

        let block = async move {
            let option = ChangeStreamOptions::builder()
                .full_document(Some(FullDocumentType::UpdateLookup))
                .build();

            let mut stream = s.group_collection().watch(None, option).await.unwrap();

            while let Ok(Some(evt)) = stream
                .try_next()
                .await
                .map_err(|e| log::error!("watch error :{:?}", e.to_string()))
            {
                let ChangeStreamEvent::<MongoContent> {
                    operation_type,
                    full_document,
                    document_key,
                    ..
                } = evt;

                match operation_type {
                    change_stream::event::OperationType::Insert
                    | change_stream::event::OperationType::Update
                    | change_stream::event::OperationType::Replace => {
                        if let Some(c) = full_document {
                            s.update_cache(c.content.service.clone(), &c).await;
                        }
                    }
                    change_stream::event::OperationType::Delete => {
                        if let Some(c) = document_key {
                            if let Ok(key) = c.get_str("_id") {
                                s.remove_cache(&key).await;
                            }
                        }
                    }
                    _ => {}
                }
            }
        };

        tokio::spawn(block);
    }

    // start renewal refresh background
    async fn remote_refresh(&mut self, ctx: Context, wg: WaitGroup) {
        let mut s = self.clone();
        let mut ctx = ctx;

        tokio::spawn(async move {
            let block = async {
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    s.service_content_renewal().await;
                }
            };
            tokio::select! {
                _ = block => {},
                _ = ctx.done() => {
                    s.service_unset().await;
                    drop(wg.clone());
                },
            }
        });
    }

    async fn twoway_refresh(&mut self, ctx: Context, wg: WaitGroup) {
        let mongodb = self.clone();
        let mut ctx = ctx;
        let mut _self = self.clone();

        let block = async move {
            let mut s = mongodb.clone();
            let block0 = async {
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    s.service_content_renewal().await;
                }
            };

            let mut s = mongodb.clone();
            let block1 = async move {
                let option = ChangeStreamOptions::builder()
                    .full_document(Some(FullDocumentType::UpdateLookup))
                    .build();

                let mut stream = s.group_collection().watch(None, option).await.unwrap();

                while let Some(evt) = stream.try_next().await.unwrap() {
                    let ChangeStreamEvent::<MongoContent> {
                        operation_type,
                        full_document,
                        document_key,
                        ..
                    } = evt;

                    match operation_type {
                        change_stream::event::OperationType::Insert
                        | change_stream::event::OperationType::Update
                        | change_stream::event::OperationType::Replace => {
                            if let Some(c) = full_document {
                                s.update_cache(c.content.service.clone(), &c).await;
                            }
                        }
                        change_stream::event::OperationType::Delete => {
                            if let Some(c) = document_key {
                                if let Ok(key) = c.get_str("_id") {
                                    s.remove_cache(&key).await;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            };
            tokio::select! {
                _ = block0 => {},
                _ = block1 => {},
                _ = ctx.done() => {
                    _self.service_unset().await;
                    drop(wg.clone());
                },
            }
        };

        tokio::spawn(block);
    }
}
