use std::fmt::Debug;

use futures::channel::{mpsc, oneshot};
use futures::future::RemoteHandle;
use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, SinkExt, Stream, StreamExt, TryFutureExt};

use common::conn::{BoxFuture, ConnPair, ConnPairVec, FuncFutTransform, FutTransform};
use common::transform_pool::transform_pool_loop;

use crypto::rand::CryptoRandom;

use identity::IdentityClient;

use database::DatabaseClient;

use proto::app_server::messages::{AppPermissions, AppServerToApp, AppToAppServer, NodeReport};
use proto::crypto::PublicKey;
use proto::net::messages::NetAddress;
use proto::proto_ser::{ProtoDeserialize, ProtoSerialize};

use connection::{create_encrypt_keepalive, create_version_encrypt_keepalive};

use timer::TimerClient;

/*
use keepalive::KeepAliveChannel;
use secure_channel::SecureChannel;
use version::VersionPrefix;
*/

use node::{
    node, ConnPairServer, IncomingAppConnection, NodeConfig, NodeError, NodeMutation, NodeState,
};

#[derive(Debug)]
pub enum NetNodeError {
    CreateThreadPoolError,
    RequestPublicKeyError,
    SpawnError,
    DatabaseIdentityMismatch,
    NodeError(NodeError),
}

#[derive(Clone)]
struct AppConnTransform<CT, TA, S> {
    conn_transform: CT,
    trusted_apps: TA,
    spawner: S,
}

impl<CT, TA, S> AppConnTransform<CT, TA, S> {
    fn new(conn_transform: CT, trusted_apps: TA, spawner: S) -> Self {
        AppConnTransform {
            conn_transform,
            trusted_apps,
            spawner,
        }
    }
}

impl<CT, TA, S> FutTransform for AppConnTransform<CT, TA, S>
where
    CT: FutTransform<
            Input = (Option<PublicKey>, ConnPairVec),
            Output = Option<(PublicKey, ConnPairVec)>,
        > + Clone
        + Send,
    TA: TrustedApps + Send + Clone,
    S: Spawn + Clone + Send + 'static,
{
    type Input = ConnPairVec;
    type Output = Option<IncomingAppConnection<NetAddress>>;

    fn transform(&mut self, conn_pair: Self::Input) -> BoxFuture<'_, Self::Output> {
        Box::pin(async move {
            let (public_key, conn_pair) = self.conn_transform.transform((None, conn_pair)).await?;

            let (mut sender, mut receiver) = conn_pair.split();

            // Obtain permissions for app (Or reject it if not trusted):
            let app_permissions: AppPermissions =
                self.trusted_apps.app_permissions(&public_key).await?;

            // Tell app about its permissions:
            sender.send(app_permissions.proto_serialize()).await.ok()?;

            let (report_sender, report_receiver) =
                oneshot::channel::<(NodeReport, oneshot::Sender<ConnPairServer<NetAddress>>)>();

            let spawner = self.spawner.clone();
            let c_spawner = self.spawner.clone();

            spawner
                .spawn(async move {
                    let _ = async move {
                        let (node_report, conn_sender) = report_receiver.await.ok()?;
                        sender.send(node_report.proto_serialize()).await.ok()?;

                        // serialization:
                        let (user_sender, mut from_user_sender) =
                            mpsc::channel::<AppServerToApp>(0);
                        let (mut to_user_receiver, user_receiver) = mpsc::channel(0);

                        // Deserialize received data
                        let _ = c_spawner.spawn(async move {
                            let _ = async move {
                                while let Some(data) = receiver.next().await {
                                    let message = AppToAppServer::proto_deserialize(&data).ok()?;
                                    to_user_receiver.send(message).await.ok()?;
                                }
                                Some(())
                            }
                            .await;
                        });

                        // Serialize sent data:
                        let _ = c_spawner.spawn(async move {
                            let _ = async move {
                                while let Some(message) = from_user_sender.next().await {
                                    // let data = serialize_app_server_to_app(&message);
                                    let data = message.proto_serialize();
                                    sender.send(data).await.ok()?;
                                }
                                Some(())
                            }
                            .await;
                        });

                        conn_sender
                            .send(ConnPair::from_raw(user_sender, user_receiver))
                            .ok()
                    }
                    .await;
                })
                .ok()?;

            Some(IncomingAppConnection {
                app_permissions: app_permissions.clone(),
                report_sender,
            })
        })
    }
}

fn transform_incoming_apps<IAC, R, TA, S>(
    incoming_app_raw_conns: IAC,
    identity_client: IdentityClient,
    rng: R,
    timer_client: TimerClient,
    trusted_apps: TA,
    max_concurrent_incoming_apps: usize,
    spawner: S,
) -> Result<
    (
        RemoteHandle<()>,
        mpsc::Receiver<IncomingAppConnection<NetAddress>>,
    ),
    NetNodeError,
>
where
    IAC: Stream<Item = ConnPairVec> + Unpin + Send + 'static,
    R: CryptoRandom + Clone + Send + Sync + 'static,
    TA: TrustedApps + Send + Clone + 'static,
    S: Spawn + Clone + Send + 'static,
{
    let conn_transform =
        create_version_encrypt_keepalive(timer_client, identity_client, rng, spawner.clone());

    let app_conn_transform = AppConnTransform::new(conn_transform, trusted_apps, spawner.clone());

    let (incoming_apps_sender, incoming_apps) = mpsc::channel(0);

    // Apply transform over every incoming app connection:
    let pool_fut = transform_pool_loop(
        incoming_app_raw_conns,
        incoming_apps_sender,
        app_conn_transform,
        max_concurrent_incoming_apps,
    )
    .map_err(|e| error!("transform_pool_loop() error: {:?}", e))
    .map(|_| ());

    // We spawn with handle here to make sure that this
    // future is dropped when this async function ends.
    let pool_handle = spawner
        .spawn_with_handle(pool_fut)
        .map_err(|_| NetNodeError::SpawnError)?;

    Ok((pool_handle, incoming_apps))
}

pub trait TrustedApps {
    /// Get the permissions of an app. Returns None if the app is not trusted at all.
    fn app_permissions<'a>(
        &'a mut self,
        app_public_key: &'a PublicKey,
    ) -> BoxFuture<'a, Option<AppPermissions>>;
}

pub async fn net_node<IAC, C, R, TA, S>(
    incoming_app_raw_conns: IAC,
    connector: C,
    timer_client: TimerClient,
    identity_client: IdentityClient,
    rng: R,
    node_config: NodeConfig,
    trusted_apps: TA,
    node_state: NodeState<NetAddress>,
    database_client: DatabaseClient<NodeMutation<NetAddress>>,
    spawner: S,
) -> Result<(), NetNodeError>
where
    IAC: Stream<Item = ConnPairVec> + Unpin + Send + 'static,
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send + 'static,
    R: CryptoRandom + Clone + Send + Sync + 'static,
    TA: TrustedApps + Send + Clone + 'static,
    S: Spawn + Clone + Send + 'static,
{
    // TODO: Move this number somewhere else?
    let max_concurrent_incoming_apps = 0x10;
    let (_pool_handle, incoming_apps) = transform_incoming_apps(
        incoming_app_raw_conns,
        identity_client.clone(),
        rng.clone(),
        timer_client.clone(),
        trusted_apps,
        max_concurrent_incoming_apps,
        spawner.clone(),
    )?;

    let conn_transform = create_version_encrypt_keepalive(
        timer_client.clone(),
        identity_client.clone(),
        rng.clone(),
        spawner.clone(),
    );

    let secure_connector = FuncFutTransform::new(move |(public_key, net_address)| {
        let mut c_connector = connector.clone();
        let mut c_conn_transform = conn_transform.clone();
        Box::pin(async move {
            let conn_pair = c_connector.transform(net_address).await?;
            let (_public_key, conn_pair) = c_conn_transform
                .transform((Some(public_key), conn_pair))
                .await?;
            Some(conn_pair)
        })
    });

    let encrypt_keepalive = create_encrypt_keepalive(
        timer_client.clone(),
        identity_client.clone(),
        rng.clone(),
        spawner.clone(),
    );

    node(
        node_config,
        identity_client,
        timer_client,
        node_state,
        database_client,
        secure_connector,
        encrypt_keepalive,
        incoming_apps,
        rng,
        spawner.clone(),
    )
    .await
    .map_err(NetNodeError::NodeError)
}
