use crossbeam::sync::WaitGroup;
use futures::{lock::Mutex, TryStreamExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio_context::context::Context;

use mongodb::{
    bson::{doc, oid::ObjectId},
    change_stream::{self, event::ChangeStreamEvent},
    options::{ChangeStreamOptions, FindOptions, FullDocumentType, IndexOptions, UpdateOptions},
    Client, IndexModel,
};

use crate::ServiceContent;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct MongoContent {
    #[serde(alias = "_id")]
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
pub struct Mongodb {
    cs: Arc<Mutex<Vec<MongoContent>>>,
    cache: Arc<Mutex<HashMap<String, Vec<MongoContent>>>>,
    client: Client,
}

impl Mongodb {
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
            cs: Arc::new(Mutex::new(vec![])),
            cache: Arc::new(Mutex::new(HashMap::new())),
            client,
        };

        s.init().await;

        s
    }

    #[inline]
    async fn init(&mut self) {
        let collection = self.collecion().await;

        let mut index_options = IndexOptions::default();
        index_options.expire_after = Some(std::time::Duration::from_secs(2));

        let index_model = IndexModel::builder()
            .keys(doc! { "time":1, })
            .options(index_options)
            .build();

        let _ = collection.create_index(index_model, None).await;
    }

    #[inline]
    async fn collecion(&self) -> mongodb::Collection<MongoContent> {
        self.client
            .database(SCHEMA_NAME)
            .collection(COLLECTION_NAME)
    }

    #[inline]
    async fn update_cache(&mut self, service: String, c: &MongoContent) {
        let mut cache = self.cache.lock().await;
        if !cache.contains_key(&service) {
            cache.insert(service, vec![c.clone()]);
            return;
        }
        let v = cache.get_mut(&service).unwrap();
        if !v.iter().any(|_c| _c.ne(c)) {
            v.push(c.clone());
        }
    }

    #[inline]
    async fn remove_cache(&mut self, id: &str) {
        let mut cache = self.cache.lock().await;

        for (_, values) in cache.iter_mut() {
            if let Some(index) = values.iter().position(|x| x.id.ne(id)) {
                values.remove(index);
            }
        }
    }

    #[inline]
    async fn renewal(&mut self) {
        let contents = self.cs.lock().await;
        for c in contents.clone().iter() {
            let id = c.id.clone();
            if let Err(e) = self.apply(&id, &c.content).await {
                log::error!("{:?}", e);
            }
        }
    }

    async fn apply<'a>(
        &self,
        id: &'a str,
        content: &'a crate::ServiceContent,
    ) -> Result<(), crate::PluginError> {
        let collection = self.collecion().await;

        collection
            .update_one(
                doc! {"service": content.service.clone(), "addr": &content.addr},
                doc! {"$set":
                    {
                    "_id" : id,
                    "service": content.service.clone(),
                    "lba": content.lba.clone(),
                    "addr": &content.addr,
                    "time": mongodb::bson::DateTime::now(),
                    },
                },
                UpdateOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|e| crate::PluginError::Error(e.to_string()))?;

        Ok(())
    }

    async fn query(&self, k: &str) -> Result<Vec<crate::ServiceContent>, crate::PluginError> {
        let mut mcs: Vec<MongoContent> = Vec::new();

        if let Ok(mut cursor) = self
            .collecion()
            .await
            .find(
                doc! { "service": k.to_string() },
                FindOptions::builder().sort(doc! { "time": -1 }).build(),
            )
            .await
            .map_err(|e| crate::PluginError::Error(e.to_string()))
        {
            while let Ok(Some(doc)) = cursor
                .try_next()
                .await
                .map_err(|e| log::error!("query decode error :{:?}", e.to_string()))
            {
                self.cache
                    .lock()
                    .await
                    .insert(doc.content.service.clone(), vec![doc.clone()]);
                mcs.push(doc);
            }
        }

        Ok(mcs.iter().map(|mc| mc.content.clone()).collect())
    }

    async fn content(&mut self, c: &crate::ServiceContent) -> String {
        let id = ObjectId::new().to_string();
        let mut contents = self.cs.lock().await;

        contents.push(MongoContent {
            id: id.clone(),
            content: c.clone(),
        });

        id
    }

    async fn unset(&mut self) {
        let contents = self.cs.lock().await;
        for c in contents.iter() {
            self.collecion()
                .await
                .delete_one(doc! {"_id":c.id.clone()}, None)
                .await
                .map_err(|e| log::error!("unset service {:?}", e))
                .unwrap();
        }
    }
}

#[crate::async_trait]
impl crate::Plugin for Mongodb {
    async fn set(&mut self, _: &str, val: crate::ServiceContent) -> Result<(), crate::PluginError> {
        let id = self.content(&val).await;
        self.apply(&id, &val).await
    }

    async fn get(&self, k: &str) -> Result<Vec<crate::ServiceContent>, crate::PluginError> {
        let cache = self.cache.lock().await;
        if let Some(v) = cache.get(k) {
            return Ok(v.iter().map(|c| c.content.clone()).collect());
        }
        self.query(k).await
    }

    async fn watch(&mut self) {
        let mut s = self.clone();
        tokio::spawn(async move {
            let option = ChangeStreamOptions::builder()
                .full_document(Some(FullDocumentType::UpdateLookup))
                .build();

            let mut stream = s.collecion().await.watch(None, option).await.unwrap();

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
                            let id = c.get("_id").unwrap().to_string();
                            if id != "" {
                                s.remove_cache(&id).await;
                            }
                        }
                    }
                    _ => {}
                }
            }
        });
    }

    // start renewal refresh background
    async fn refresh(&mut self, ctx: Context, wg: WaitGroup) {
        let mut s = self.clone();
        let mut ctx = ctx;

        tokio::spawn(async move {
            let block = async {
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    s.renewal().await;
                }
            };
            tokio::select! {
                _ = block => {},
                _ = ctx.done() => {
                    s.unset().await;
                    drop(wg.clone());
                },
            }
        });
    }
}
