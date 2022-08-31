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

use crate::Content;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct MongoContent {
    #[serde(alias = "_id")]
    id: ObjectId,

    content: Content,
}

impl PartialEq for MongoContent {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

static SCHEMA_NAME: &str = "crossgate";
static COLLECTION_NAME: &str = "discovery";

#[derive(Debug, Clone)]
pub struct Mongodb {
    c: Arc<Mutex<MongoContent>>,
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

        let mongodb_content = MongoContent {
            id: ObjectId::new(),
            content: crate::Content {
                service: "".to_string(),
                lba: "".to_string(),
                addr: "127.0.0.1:0".parse().unwrap(),
            },
        };

        let mut s = Self {
            c: Arc::new(Mutex::new(mongodb_content)),
            cache: Arc::new(Mutex::new(HashMap::new())),
            client,
        };

        s.init().await;

        s
    }

    #[inline]
    async fn init(&mut self) {
        let collection = self.get_collection().await;

        let mut index_options = IndexOptions::default();
        index_options.expire_after = Some(std::time::Duration::from_secs(2));

        let index_model = IndexModel::builder()
            .keys(doc! { "time":1, })
            .options(index_options)
            .build();

        let _ = collection.create_index(index_model, None).await;
    }

    #[inline]
    async fn get_collection(&self) -> mongodb::Collection<MongoContent> {
        self.client
            .database(SCHEMA_NAME)
            .collection(COLLECTION_NAME)
    }

    #[inline]
    async fn update_cache(&mut self, service: String, c: &MongoContent) {
        if self.cache.lock().await.get(&service).is_none() {
            self.cache.lock().await.insert(service, vec![c.clone()]);
        } else {
            let mut cache = self.cache.lock().await;
            let v = cache.get_mut(&service).unwrap();
            if !v.iter().any(|_c| _c == c) {
                v.push(c.clone());
            }
        }
    }

    #[inline]
    async fn remove_cache(&mut self, id: String) {
        for (_, v) in self.cache.lock().await.iter_mut() {
            v.retain(|c| c.id.to_string() != id);
        }
    }

    #[inline]
    async fn renewal(&mut self) {
        let c = self.c.lock().await.clone();
        if let Err(e) = self.apply(c.id.clone(), &c.content).await {
            log::error!("{:?}", e);
        }
    }

    async fn apply(
        &mut self,
        uuid: ObjectId,
        val: &crate::Content,
    ) -> Result<(), crate::PluginError> {
        let collection = self.get_collection().await;

        collection
            .update_one(
                doc! {"content.service": val.service.clone(), "content.addr": &val.addr},
                doc! {"$set":
                    {
                    "_id" : uuid,
                    "content.service": val.service.clone(),
                    "content.lba": val.lba.clone(),
                    "content.addr": &val.addr,
                    "time": mongodb::bson::DateTime::now(),
                    },
                },
                UpdateOptions::builder().upsert(true).build(),
            )
            .await
            .map_err(|e| crate::PluginError::Error(e.to_string()))?;

        Ok(())
    }

    async fn query(&self, k: &str) -> Result<Vec<crate::Content>, crate::PluginError> {
        let c = self.get_collection().await;

        let filter = doc! { "content.service": k.to_string() };
        let option = FindOptions::builder().sort(doc! { "time": -1 }).build();

        let mut result = Vec::new();
        if let Ok(mut cursor) = c
            .find(filter, option)
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
                result.push(doc.content);
            }
        }

        Ok(result)
    }

    async fn set_content(&mut self, c: &crate::Content) -> ObjectId {
        let id = ObjectId::new();
        self.c.lock().await.clone_from(&MongoContent {
            id: id.clone(),
            content: c.clone(),
        });

        id
    }

    async fn unset(&mut self) {
        let c = self.get_collection().await;
        c.delete_one(doc! {"_id":self.c.lock().await.id}, None)
            .await
            .map_err(|e| log::error!("unset service {:?}", e))
            .unwrap();
    }
}

#[crate::async_trait]
impl crate::Plugin for Mongodb {
    async fn set(&mut self, k: &str, val: crate::Content) -> Result<(), crate::PluginError> {
        let id = self.set_content(&val).await;
        self.apply(id, &val).await
    }

    async fn get(&self, k: &str) -> Result<Vec<crate::Content>, crate::PluginError> {
        let cache = self.cache.lock().await;
        if let Some(v) = cache.get(k) {
            return Ok(v.iter().map(|c| c.content.clone()).collect());
        }
        self.query(k).await
    }

    async fn watch(&mut self) {
        let mut s = self.clone();
        tokio::spawn(async move {
            let collection = s.get_collection().await;

            let option = ChangeStreamOptions::builder()
                .full_document(Some(FullDocumentType::UpdateLookup))
                .build();

            let mut stream = collection.watch(None, option).await.unwrap();

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
                            let id = c.get("_id").unwrap().as_object_id().unwrap().to_string();
                            s.remove_cache(id).await;
                        }
                    }
                    _ => {}
                }
            }
        });
    }

    // start renewal background
    async fn renewal(&mut self, ctx: Context, wg: WaitGroup) {
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
