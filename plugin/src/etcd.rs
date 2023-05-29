// use crossbeam::sync::WaitGroup;
// use tokio_context::context::Context;

pub struct Etcd {
    // 实现当前插件为本地cache， 可以提高性能，将数据异步写入etcd，并且可以实现自动更新
}

// #[crate::async_trait]
// impl crate::Plugin for Etcd {
//     async fn set(&mut self, k: &str, val: crate::Content) -> Result<(), crate::PluginError> {
//         log::info!("set key {},val {:?}", k, val);
//         // TODO: 添加 数据，存储前需要加锁
//         // 查询是否已有的key,如果没有，那么将数据插入到数据库中,超时为N秒,每隔少于N秒需要做续期操作
//         Ok(())
//     }

//     async fn get(&self, k: &str) -> Result<Vec<crate::Content>, crate::PluginError> {
//         // 查询符合k的多个服务，返回Content 的 endpoints有一个或者多个
//         Err(crate::PluginError::RecordNotFound)
//     }

//     async fn watch(&mut self) {}

//     async fn refresh(&mut self, ctx: Context, wg: WaitGroup) {}
// }
