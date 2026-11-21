//! Multiplayer session: a network thread that connects to a server, logs in,
//! runs the configuration phase and then forwards decoded play packets to the
//! game thread. Keep-alives are answered on the network thread; pings wait for
//! the game thread, like ClientCommonPacketListenerImpl.handlePing, so a pong
//! cannot overtake an earlier world update or teleport acknowledgement.
//! Online-mode servers need the Microsoft account the launcher signed in
//! (crate::auth): the login joins the server through the session service and
//! encrypts the connection (ClientHandshakePacketListenerImpl.handleHello).
use crate::auth::AuthSession;
use horizon_protocol::{
    PROTOCOL_VERSION, connection::{ConnReader, ConnWriter, Connection, Counters},
    crypto::{generate_secret, offline_uuid, server_hash},
    packets::{Empty, configuration as cfg, handshake, login, play},
    registry::Dynamic, rsa::RsaPublicKey, servers::parse_address,
    text::{Language, plain, translatable},
};
use horizon_world::nbt::{JavaString, Tag};
use std::{
    collections::HashMap, path::{Path, PathBuf},
    sync::{
        Arc, Mutex, atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread::JoinHandle, time::Duration,
};
use horizon_hooks::tunnel;
/// `CommonListenerCookie.serverCookies`: cookies the server asked the client
/// to store (`ClientboundStoreCookiePacket`), answered to
/// `ClientboundCookieRequestPacket`. A `transfer` carries them to the next
/// connection (`TransferState`), so a proxy's backend switch keeps them.
pub type Cookies = Arc<Mutex<HashMap<String, Vec<u8>>>>;
/// A recording of the session's packets: the tap a replay recorder plugs into. Replay recording is a
/// platform feature (docs/layers.md, "Controllers, splitscreen, replays and UI pages"); the engine only
/// hands it the packets: the network thread what it receives, the game thread what the client makes for
/// the recording itself (its own movement, pose and swings).
pub trait PacketRecorder: Send {
    /// Whether it still takes packets (false once stopped or failed).
    fn is_active(&self) -> bool;
    /// A play packet the network thread received, decoded, with its id and payload as they came.
    /// The recorder decides what it keeps.
    fn received(&self, packet: &play::Clientbound, id: i32, payload: &[u8]);
    /// A packet the client made for the recording, encoded, stamped now.
    fn record_packet(&self, id: i32, payload: &[u8]);
    fn status(&self) -> RecorderStatus;
    /// Finishes the recording; the file it wrote.
    fn stop(self: Box<Self>) -> std::io::Result<PathBuf>;
}
/// Where a recording stands.
#[derive(Clone, Debug, PartialEq)]
pub struct RecorderStatus {
    pub active: bool,
    pub path: PathBuf,
    pub elapsed: Duration,
    pub packet_count: u64,
    pub bytes_written: u64,
    pub error: Option<String>,
}
/// The session's recorder, shared with its network thread.
type RecorderSlot = Arc<Mutex<Option<Box<dyn PacketRecorder>>>>;
/// What the network thread reports to the game.
pub enum Event {
    /// Progress text for the connecting screen.
    Status(String),
    /// The server's `minecraft:brand` custom payload.
    Brand(String),
    /// The server runs mods' quiet commands (`horizon_protocol::quiet`).
    QuietCommands,
    /// The server carries mod messages (`horizon_protocol::mod_net`).
    ModNet,
    /// The server samples far terrain for this client (`lod_net`).
    FarTerrain(crate::lod_net::Offer),
    /// ClientConfigurationPacketListenerImpl.handleEnabledFeatures.
    Features(Vec<String>),
    /// Configuration finished: the play state begins with these registries.
    Joined { registries: Arc<Dynamic>, uuid: u128, name: String },
    Packet(Box<play::Clientbound>),
    /// A packet that failed to decode (skipped, like vanilla's skippable packets).
    DecodeError(String),
    /// The server sent us back to configuration (`ClientboundStartConfigurationPacket`):
    /// the game shows `ServerReconfigScreen` until the server logs it in again.
    Reconfiguring,
    /// The server sent us back to configuration and finished it again.
    Reconfigured(Arc<Dynamic>),
    /// ClientboundResourcePackPushPacket, in configuration or in play.
    PackPush(cfg::ResourcePackPush),
    /// ClientboundResourcePackPopPacket: one pack, or every pack.
    PackPop(Option<u128>),
    /// `show_dialog` in configuration: the dialog itself.
    ShowDialog(horizon_world::nbt::Tag),
    /// `clear_dialog` in configuration.
    ClearDialog,
    /// ClientboundCodeOfConductPacket: the game shows it and answers with
    /// `Session::accept_code_of_conduct` or a disconnect; until then the
    /// server waits in configuration.
    CodeOfConduct(String),
    /// `ClientboundTransferPacket`: the server sent us to another address.
    /// The connection ends right after; the game reconnects there with the
    /// stored cookies.
    Transfer { host: String, port: i32 },
    /// `ClientboundServerLinksPacket`: the links the pause menu's "Server
    /// Links..." lists (ClientCommonPacketListenerImpl.handleServerLinks).
    ServerLinks(Vec<cfg::ServerLink>),
    /// The connection ended. `from_server`: the server gave the reason (or
    /// the connection was refused for good), rather than the connection
    /// failing (auto-reconnect retries only failures and shutdowns).
    Disconnected {
        reason: String,
        /// The reason's translation key, when it had one
        /// (`reconnect::allowed` classifies by it).
        key: Option<String>,
        from_server: bool,
    },
}
pub struct Session {
    events: Receiver<Event>,
    writer: Arc<Mutex<Option<ConnWriter>>>,
    /// The connection is in configuration (else in play). Changed only
    /// with `writer` locked, as the packet that switches is sent.
    configuring: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub address: String,
    pub name: String,
    pub counters: Arc<Mutex<Option<Arc<Counters>>>>,
    /// The cookies this server stored on us (`ClientboundStoreCookiePacket`).
    pub cookies: Cookies,
    /// HORIZON_NET_DEBUG=1 logs every packet sent from the game thread.
    debug: bool,
    /// The far terrain requests waiting for the server (`lod_net`).
    far: Arc<crate::lod_net::Inbox>,
    /// The recording running, if any.
    recorder: RecorderSlot,
    /// This connection's number, new for each one ([`Session::serial`]).
    serial: u64,
}
/// The next [`Session::serial`].
static NEXT_SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
fn next_serial() -> u64 {
    NEXT_SERIAL.fetch_add(1, Ordering::Relaxed)
}
impl Session {
    /// A number of its own for each connection, never 0: entity ids are per connection, so what
    /// is kept by id goes when it changes (`horizon_hooks::players::PlayerModels::connection`).
    pub fn serial(&self) -> u64 {
        self.serial
    }
    /// The negotiated wire release, once the connection has been established.
    pub fn server_version(&self) -> horizon_protocol::adapter::ServerVersion {
        self.writer
            .lock()
            .unwrap()
            .as_ref()
            .map_or(
                horizon_protocol::adapter::ServerVersion::Current,
                ConnWriter::server_version,
            )
    }
    /// Secure-chat enforcement synchronized later than login on older servers.
    pub fn legacy_secure_chat_enforcement(&self) -> Option<bool> {
        self.writer
            .lock()
            .unwrap()
            .as_ref()
            .and_then(ConnWriter::legacy_secure_chat_enforcement)
    }
    /// Start connecting in the background, as `auth`'s account when there is one.
    pub fn connect(
        address: &str,
        name: &str,
        auth: Option<Arc<AuthSession>>,
        info: cfg::ClientInformation,
        lang: Arc<Language>,
        resources: PathBuf,
    ) -> Self {
        Self::start(address, name, auth, info, lang, None, None, resources, false)
    }
    /// `ClientHandshakePacketListenerImpl.handleHello`: LAN entries can continue
    /// after session authentication fails; the server still decides whether to admit them.
    /// Only for entries selected from LAN discovery, not addresses classified by IP.
    pub fn connect_lan(
        address: &str,
        name: &str,
        auth: Option<Arc<AuthSession>>,
        info: cfg::ClientInformation,
        lang: Arc<Language>,
        resources: PathBuf,
    ) -> Self {
        Self::start(address, name, auth, info, lang, None, None, resources, true)
    }
    /// `ConnectScreen.startConnecting` for a `transfer`: the cookies the
    /// server stored carry to the new connection (`TransferState`).
    pub fn connect_transferred(
        address: &str,
        name: &str,
        auth: Option<Arc<AuthSession>>,
        info: cfg::ClientInformation,
        lang: Arc<Language>,
        cookies: Cookies,
        resources: PathBuf,
    ) -> Self {
        Self::start(
            address,
            name,
            auth,
            info,
            lang,
            None,
            Some(cookies),
            resources,
            false,
        )
    }
    /// Join an integrated server through memory (`Connection.connectToLocalServer`). The
    /// hello still names the signed-in account (`Minecraft.handleClientLogin`'s
    /// `ServerboundHelloPacket(getUser().getName(), getUser().getProfileId())`), so the
    /// world's `players/data/<uuid>.dat` and `singleplayer_uuid` are the account's.
    pub fn connect_local(
        pipe: horizon_protocol::connection::Pipe,
        name: &str,
        auth: Option<Arc<AuthSession>>,
        info: cfg::ClientInformation,
        lang: Arc<Language>,
        resources: PathBuf,
    ) -> Self {
        {
            fn sunset_client_local() {
                crate::net::sunset_client_brand::next(None);
            }
            sunset_client_local()
        };
        Self::start("local", name, auth, info, lang, Some(pipe), None, resources, false)
    }
    /// A lightweight session for studio replay playback without background network IO.
    pub fn replay(name: &str) -> Self {
        let (_send, events) = channel();
        Self {
            events,
            writer: Arc::new(Mutex::new(None)),
            configuring: Arc::new(AtomicBool::new(false)),
            thread: None,
            address: "studio-replay".to_owned(),
            name: name.to_owned(),
            counters: Arc::new(Mutex::new(None)),
            cookies: Default::default(),
            debug: false,
            far: Arc::new(crate::lod_net::Inbox::default()),
            recorder: Arc::new(Mutex::new(None)),
            serial: next_serial(),
        }
    }
    fn start(
        address: &str,
        name: &str,
        auth: Option<Arc<AuthSession>>,
        info: cfg::ClientInformation,
        lang: Arc<Language>,
        local: Option<horizon_protocol::connection::Pipe>,
        cookies: Option<Cookies>,
        resources: PathBuf,
        lan: bool,
    ) -> Self {
        let (send, events) = channel();
        let writer = Arc::new(Mutex::new(None));
        let configuring = Arc::new(AtomicBool::new(true));
        let counters = Arc::new(Mutex::new(None));
        let transferred = cookies.is_some();
        let cookies = cookies.unwrap_or_default();
        let (thread_writer, thread_counters, thread_cookies) = (
            writer.clone(),
            counters.clone(),
            cookies.clone(),
        );
        let thread_configuring = configuring.clone();
        let far = Arc::new(crate::lod_net::Inbox::default());
        let thread_far = far.clone();
        let recorder = Arc::new(Mutex::new(None));
        let thread_recorder = recorder.clone();
        let (address_owned, name_owned) = (address.to_owned(), name.to_owned());
        let thread = std::thread::Builder::new()
            .name("horizon-net".into())
            .spawn(move || {
                horizon_threads::favour_current();
                let result = run(
                    local,
                    &address_owned,
                    &name_owned,
                    auth.as_deref(),
                    info,
                    &lang,
                    &send,
                    &thread_writer,
                    &thread_configuring,
                    &thread_counters,
                    &thread_cookies,
                    &thread_far,
                    transferred,
                    &resources,
                    lan,
                    &thread_recorder,
                );
                thread_far.cancel();
                let from_server = result.is_ok();
                let why = result.unwrap_or_else(|e| e);
                if let Some(w) = thread_writer.lock().unwrap().take() {
                    w.shutdown();
                }
                let _ = send
                    .send(Event::Disconnected {
                        reason: why.text,
                        key: why.key,
                        from_server,
                    });
            })
            .ok();
        let debug = std::env::var_os("HORIZON_NET_DEBUG").is_some();
        Self {
            events,
            writer,
            configuring,
            thread,
            address: address.to_owned(),
            name: name.to_owned(),
            counters,
            cookies,
            debug,
            far,
            recorder,
            serial: next_serial(),
        }
    }
    /// Everything received since the last poll.
    pub fn poll(&self) -> Vec<Event> {
        ({
            fn sunset_packet_tap(session: &Session, value: Vec<Event>) -> Vec<Event> {
                crate::net::sunset_packets::note(session, &value);
                if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
                    let value = crate::net::sunset_packets::backtrack_hold(
                        session,
                        value,
                    );
                    return crate::net::sunset_packets::hold(session, value);
                }
                let value = crate::net::sunset_packets::trap_filter(value);
                let value = crate::net::sunset_packets::backtrack_hold(session, value);
                let value = crate::net::sunset_packets::hold(session, value);
                crate::net::sunset_packets::seen(&value);
                crate::net::sunset_client_brand::seen(&value);
                crate::net::sunset_packets::gui_move_hold(value)
            }
            sunset_packet_tap
        })(&*self, self.events.try_iter().collect())
    }
    /// Starts recording into `recorder`. `initialize` runs first, while the network thread
    /// can't record, so what it writes (a snapshot of the world) comes before any live packet.
    pub fn start_recording<R: PacketRecorder + 'static>(
        &self,
        recorder: R,
        initialize: impl FnOnce(&R),
    ) {
        let mut slot = self.recorder.lock().unwrap();
        initialize(&recorder);
        *slot = Some(Box::new(recorder));
    }
    /// Stop recording and finalize the `.mcpr` archive.
    pub fn stop_recording(&self) -> Option<std::io::Result<PathBuf>> {
        let rec = self.recorder.lock().unwrap().take()?;
        Some(rec.stop())
    }
    /// Whether recording is currently active.
    pub fn is_recording(&self) -> bool {
        self.recorder.lock().unwrap().as_ref().is_some_and(|r| r.is_active())
    }
    /// Status of the active recording, if any.
    pub fn recorder_status(&self) -> Option<RecorderStatus> {
        self.recorder.lock().unwrap().as_ref().map(|r| r.status())
    }
    /// Record an encoded Clientbound packet into the active recording session at current elapsed time.
    pub fn record_clientbound(
        &self,
        packet: &horizon_protocol::packets::play::Clientbound,
    ) {
        if self.is_recording() {
            let encoded = packet.encode();
            let mut reader = horizon_protocol::buf::Reader::new(&encoded);
            if let Ok(id) = reader.varint()
                && let Some(rec) = self.recorder.lock().unwrap().as_ref()
            {
                rec.record_packet(id, reader.rest());
            }
        }
    }
    /// Send a play packet; silently dropped once the connection is gone,
    /// and while the server has the connection back in configuration (a
    /// proxy switching servers). Vanilla's `handleConfigurationStart` clears
    /// the level before it acknowledges, so no play packet follows the
    /// acknowledgement; one that did would reach the server as a
    /// configuration packet, and Velocity drops such a client.
    pub fn send(&self, packet: play::Serverbound) {
        if self.debug
            && !matches!(
                packet, play::Serverbound::ClientTickEnd(_) |
                play::Serverbound::MovePlayerPos(_) |
                play::Serverbound::MovePlayerPosRot(_)
            )
        {
            eprintln!("[net] -> {packet:?}");
        }
        let mut writer = self.writer.lock().unwrap();
        if self.configuring.load(Ordering::Relaxed) {
            return;
        }
        if let Some(w) = writer.as_mut() && w.send(&packet.encode()).is_err() {
            w.shutdown();
        }
    }
    /// Far terrain the server samples, as it offered (`Event::FarTerrain`):
    /// biomes named by this client's indices, block states below `states`.
    pub fn far_terrain(
        &self,
        offer: crate::lod_net::Offer,
        biomes: HashMap<String, u16>,
        states: u32,
    ) -> Arc<crate::lod_net::RemoteSource> {
        crate::lod_net::RemoteSource::new(self.far.clone(), offer, biomes, states)
    }
    /// The far terrain requests its samplers queued go to the server, from
    /// the game thread, among its other packets.
    pub fn send_far_terrain(&self) {
        for data in self.far.requests() {
            self.send(
                play::Serverbound::CustomPayload(cfg::CustomPayload {
                    channel: crate::lod_net::CHANNEL.into(),
                    data,
                }),
            );
        }
    }
    /// The far terrain made with [`Self::far_terrain`] ends: its samplers
    /// stop waiting for the server.
    pub fn cancel_far_terrain(&self) {
        self.far.cancel();
    }
    /// The connection is configured again (a proxy's next server): requests
    /// the old server held never come back, so their slots are freed.
    pub fn reset_far_terrain(&self) {
        self.far.reset();
    }
    /// ServerboundResourcePackPacket, in whichever phase the connection is.
    pub fn send_pack_status(&self, id: u128, action: i32) {
        let status = cfg::ResourcePackResponse {
            id,
            action,
        };
        let mut writer = self.writer.lock().unwrap();
        let body = if self.configuring.load(Ordering::Relaxed) {
            cfg::Serverbound::ResourcePack(status).encode()
        } else {
            play::Serverbound::ResourcePack(status).encode()
        };
        if let Some(w) = writer.as_mut() && w.send(&body).is_err() {
            w.shutdown();
        }
    }
    /// ServerboundCustomClickActionPacket (a dialog's answer), in whichever
    /// phase the connection is.
    pub fn send_custom_click(&self, id: &str, payload: Option<horizon_world::nbt::Tag>) {
        let click = cfg::CustomClickAction {
            id: id.to_owned(),
            payload,
        };
        let mut writer = self.writer.lock().unwrap();
        let body = if self.configuring.load(Ordering::Relaxed) {
            cfg::Serverbound::CustomClickAction(click).encode()
        } else {
            play::Serverbound::CustomClickAction(click).encode()
        };
        if let Some(w) = writer.as_mut() && w.send(&body).is_err() {
            w.shutdown();
        }
    }
    /// ServerboundAcceptCodeOfConductPacket, sent in configuration.
    pub fn accept_code_of_conduct(&self) {
        let mut writer = self.writer.lock().unwrap();
        if let Some(w) = writer.as_mut()
            && w.send(&cfg::Serverbound::AcceptCodeOfConduct(Empty).encode()).is_err()
        {
            w.shutdown();
        }
    }
    /// `Connection.disconnect`: the socket closes and the game goes on at once.
    /// The reader thread is left to end on its own: on Windows a shutdown
    /// doesn't wake a read already waiting, so joining it held the window until
    /// the server dropped the connection, many seconds later.
    pub fn disconnect(&mut self) {
        if let Some(w) = self.writer.lock().unwrap().take() {
            w.shutdown();
        }
        drop(self.thread.take());
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.disconnect();
    }
}
/// An address of the form `friend:K7QM-3XPD` joins a friend's world through
/// its invite code (Play with Friends, through a hook's [`tunnel::Resolver`]):
/// the session goes through a tunnel the client reaches on a loopback port, so
/// everything after is a plain join.
pub const FRIEND: &str = tunnel::PREFIX;
/// Whether `code` is an invite code; with `full`, only a full one (a short
/// relay code could also be a host name). Without a resolver nothing is one.
pub fn is_invite(code: &str, full: bool) -> bool {
    horizon_hooks::engine::tunnel().is_some_and(|r| r.is_code(code, full))
}
/// Whether an invite code can be joined: a resolver is registered (horizon-official registers
/// horizon-friends'). The title screen's Friends icon, which opens Join a Friend, is inactive
/// without one, as the pause menu's is without a page to share from.
pub fn joins_friends() -> bool {
    #[cfg(test)]
    if JOINS_FRIENDS_IN_TEST.get() {
        return true;
    }
    horizon_hooks::engine::tunnel().is_some()
}
#[cfg(test)]
thread_local! {
    #[doc =
    " A test's stand-in for a registered resolver: the registry is the process's and sealed."]
    pub (crate) static JOINS_FRIENDS_IN_TEST : std::cell::Cell < bool > = const {
    std::cell::Cell::new(false) };
}
/// An address as players and logs see it: a friend's invite code stays
/// out of it (its secret is in there).
pub fn shown_address(address: &str) -> String {
    match address.strip_prefix(FRIEND) {
        Some(code) => {
            match horizon_hooks::engine::tunnel().and_then(|r| r.nameplate(code)) {
                Some(nameplate) => format!("Friend's world ({nameplate}-****)"),
                None => "Friend's world".to_owned(),
            }
        }
        None => address.to_owned(),
    }
}
/// The relay the player set (`friendsRelay` in horizon.txt, beside
/// options.txt), else the resolver's (`HORIZON_RELAY_URL`). None by default.
pub fn friends_relay_setting() -> Option<String> {
    let dir = std::env::current_dir().ok()?;
    crate::ui_theme::preference(&dir, "friendsRelay")
        .filter(|r| !r.is_empty())
        .or_else(|| horizon_hooks::engine::tunnel().and_then(|r| r.relay_from_env()))
}
/// Remember the relay typed on Join a friend (empty forgets it).
pub fn save_friends_relay(relay: &str) {
    if let Ok(dir) = std::env::current_dir() {
        crate::ui_theme::set_preference(
            &dir,
            "friendsRelay",
            (!relay.is_empty()).then_some(relay),
        );
    }
}
/// Reach the friend's world from the code (directly, or through a relay)
/// and open the tunnel: the loopback address to connect to, and the token to send on it first.
/// Without a resolver (a build without Play with Friends) the address is refused, never retried.
fn join_friend(
    code: &str,
    lang: &Language,
    send: &Sender<Event>,
) -> Result<(String, u16, Vec<u8>), Reason> {
    let Some(resolver) = horizon_hooks::engine::tunnel() else {
        return Err(
            Reason::keyed_arg(
                "horizon.multiplayer.invalid_address",
                lang,
                &shown_address(&format!("{FRIEND}{code}")),
            ),
        );
    };
    let mut progress = |p: tunnel::Progress| {
        let key = match p {
            tunnel::Progress::Finding => "horizon.multiplayer.finding_friend",
            tunnel::Progress::Connected { direct: true } => {
                "horizon.multiplayer.connected_friend_direct"
            }
            tunnel::Progress::Connected { direct: false } => {
                "horizon.multiplayer.connected_friend_relay"
            }
        };
        status(send, &tr(lang, key));
    };
    let opened = resolver
        .open(code, friends_relay_setting(), &mut progress)
        .map_err(Reason::plain)?;
    Ok(("127.0.0.1".to_owned(), opened.port, opened.token))
}
fn status(send: &Sender<Event>, text: &str) {
    let _ = send.send(Event::Status(text.to_owned()));
}
/// Shown when an online-mode server asks a client without an account to authenticate.
pub const NOT_SIGNED_IN: &str = "This server is in online mode. Sign in with your Microsoft account in the Horizon launcher to join it.";
/// A translation, or the key itself (Language.getOrDefault).
fn tr(lang: &Language, key: &str) -> String {
    lang.get(key).cloned().unwrap_or_else(|| key.to_owned())
}
/// Why a connection ended: the shown text and, when the reason was a
/// translatable component or a known Horizon error, its translation key —
/// the auto-reconnect classifies by key (`reconnect::allowed`), the
/// English keyword scan of the text is the fallback for plain reasons.
#[derive(Clone)]
pub struct Reason {
    pub text: String,
    pub key: Option<String>,
}
impl Reason {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            key: None,
        }
    }
    /// A vanilla key's text in the session's language, with the key.
    fn keyed(key: &str, lang: &Language) -> Self {
        Self {
            text: tr(lang, key),
            key: Some(key.to_owned()),
        }
    }
    /// A Horizon key's text with a `%s` argument filled.
    fn keyed_arg(key: &str, lang: &Language, arg: &str) -> Self {
        let mut r = Self::keyed(key, lang);
        r.text = r.text.replacen("%s", arg, 1);
        r
    }
    /// A server's disconnect component: the resolved text and the
    /// `translate` key when the component named one (servers' literal
    /// kicks have none; their text is the server's own language).
    fn component(reason_json: &str, lang: &Language) -> Self {
        match serde_json::from_str::<serde_json::Value>(reason_json) {
            Ok(v) => {
                Self {
                    text: json_text(&v, lang),
                    key: v["translate"].as_str().map(str::to_owned),
                }
            }
            Err(_) => Self::plain(reason_json),
        }
    }
    /// The play/configuration phases' disconnect: the reason arrives as
    /// an NBT component.
    fn tag(tag: &Tag, lang: &Language) -> Self {
        let key = match tag {
            Tag::Compound(map) => {
                map.get(&JavaString::from("translate"))
                    .and_then(|t| match t {
                        Tag::String(s) => s.to_string().ok(),
                        _ => None,
                    })
            }
            _ => None,
        };
        Self {
            text: plain(tag, lang),
            key,
        }
    }
}
/// `ConnectScreen.connect`'s log line, as vanilla writes it from its "Server Connector #N" thread:
/// the address as the player gave it, and the port (25565 when none was given), before any SRV
/// lookup. Other launchers read the last one from `latest.log` to offer the server again.
fn connecting_line(host: &str, port: u16) -> String {
    format!("Connecting to {host}, {port}")
}
fn log_connecting(host: &str, port: u16) {
    static CONNECTORS: AtomicU32 = AtomicU32::new(0);
    let n = CONNECTORS.fetch_add(1, Ordering::Relaxed) + 1;
    crate::logging::write_as(
        &format!("Server Connector #{n}"),
        log::Level::Info,
        &connecting_line(host, port),
    );
}
/// `Connection$1.initChannel`: a client socket's pipeline starts with
/// `ReadTimeoutHandler(30)`. A server sends a keep-alive every 15 s, so one
/// that sends nothing for 30 s has frozen or dropped off the network.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// `ConnectScreen.startConnecting`: the socket to a server, with `read_timeout`
/// ([`READ_TIMEOUT`]) on every read. The friends relay supplies a concrete
/// endpoint, not a Minecraft hostname, so it skips the SRV lookup. Returns the
/// resolved host and port for `ConnectScreen`'s login handshake.
fn connect_tcp(
    host: &str,
    port: u16,
    relayed: bool,
    read_timeout: Duration,
) -> Result<(Connection, String, u16), String> {
    let (c, host, port) = if relayed {
        Connection::connect(host, port, Duration::from_secs(10))
            .map(|c| (c, host.to_owned(), port))
    } else {
        Connection::connect_server_with_address(host, port, Duration::from_secs(10))
    }
        .map_err(|e| e.to_string())?;
    c.reader.set_read_timeout(Some(read_timeout)).map_err(|e| e.to_string())?;
    Ok((c, host, port))
}
/// Whether joining servers of older releases is on (roadmap `older-servers`): off by default while the
/// adapter is incomplete; `HORIZON_OLDER_SERVERS=1` turns it on.
pub fn older_servers_enabled() -> bool {
    std::env::var_os("HORIZON_OLDER_SERVERS").is_some_and(|v| v == "1")
}
/// The older release a server speaks, by its status ping, when Horizon can translate it; `None` for the
/// pinned version, an unknown one, a failed ping, or with the feature off.
fn older_server(address: &str) -> Option<horizon_protocol::adapter::ServerVersion> {
    use horizon_protocol::adapter::ServerVersion;
    if !older_servers_enabled() {
        return None;
    }
    let (json, _) = ping(address, Duration::from_secs(5)).ok()?;
    let protocol = i32::try_from(json["version"]["protocol"].as_i64()?).ok()?;
    ServerVersion::from_protocol(protocol).filter(|v| *v != ServerVersion::Current)
}
/// `Connection.exceptionCaught`: a read that timed out ends with
/// `disconnect.timeout`, any other failure with `disconnect.genericReason`
/// ("Internal Exception: ..."); a stream the server closed ends with
/// `disconnect.endOfStream` (`Connection.channelInactive`).
fn connection_lost(e: horizon_protocol::Error, lang: &Language) -> String {
    if e.is_timed_out() {
        tr(lang, "disconnect.timeout")
    } else if e.is_end_of_stream() {
        tr(lang, "disconnect.endOfStream")
    } else {
        plain(
            &translatable(
                "disconnect.genericReason",
                &[&format!("Internal Exception: {e}")],
            ),
            lang,
        )
    }
}
/// `ClientHandshakePacketListenerImpl.handleHello`: the whole connection.
/// Ok: why it ended for good (the server's reason, or
/// a refusal no retry fixes); Err: the connection failed.
fn run(
    local: Option<horizon_protocol::connection::Pipe>,
    address: &str,
    name: &str,
    auth: Option<&AuthSession>,
    info: cfg::ClientInformation,
    lang: &Language,
    send: &Sender<Event>,
    shared_writer: &Mutex<Option<ConnWriter>>,
    configuring: &AtomicBool,
    shared_counters: &Mutex<Option<Arc<Counters>>>,
    cookies: &Cookies,
    far: &crate::lod_net::Inbox,
    transferred: bool,
    resources: &Path,
    lan: bool,
    recorder: &RecorderSlot,
) -> Result<Reason, Reason> {
    let (host, port, mut c) = if let Some(pipe) = local {
        (address.to_owned(), 0, Connection::local(pipe))
    } else {
        let (host, port, token) = match address.strip_prefix(FRIEND) {
            Some(code) => join_friend(code, lang, send)?,
            None => {
                let Some(target) = parse_address(address) else {
                    return Err(
                        Reason::keyed_arg(
                            "horizon.multiplayer.invalid_address",
                            lang,
                            address,
                        ),
                    );
                };
                status(send, &tr(lang, "connect.connecting"));
                log_connecting(&target.0, target.1);
                (target.0, target.1, Vec::new())
            }
        };
        let (mut c, host, port) = connect_tcp(
                &host,
                port,
                address.starts_with(FRIEND),
                READ_TIMEOUT,
            )
            .map_err(Reason::plain)?;
        if !token.is_empty() {
            c.writer.write_raw(&token).map_err(|e| Reason::plain(e.to_string()))?;
        }
        if !address.starts_with(FRIEND) && let Some(version) = older_server(address) {
            log::info!("Joining {address} as a {} server", version.name());
            c.adapt(version);
        }
        (host, port, c)
    };
    *shared_counters.lock().unwrap() = Some(c.reader.counters.clone());
    let e = |e: horizon_protocol::Error| Reason::plain(connection_lost(e, lang));
    c.send(
            &handshake::Serverbound::Intention(handshake::Intention {
                    protocol: PROTOCOL_VERSION,
                    host: host.clone(),
                    port,
                    intent: if transferred {
                        handshake::TRANSFER
                    } else {
                        handshake::LOGIN
                    },
                })
                .encode(),
        )
        .map_err(e)?;
    let uuid = auth.map_or_else(|| offline_uuid(name), |a| a.uuid);
    c.send(
            &login::Serverbound::Hello(login::ServerHello {
                    name: name.to_owned(),
                    uuid,
                })
                .encode(),
        )
        .map_err(e)?;
    let profile = loop {
        let (id, payload) = c.recv().map_err(e)?;
        match login::Clientbound::decode(id, &payload).map_err(e)? {
            login::Clientbound::Compression(p) => c.set_compression(p.threshold),
            login::Clientbound::LoginFinished(p) => {
                c.send(&login::Serverbound::LoginAcknowledged(Empty).encode())
                    .map_err(e)?;
                break p.profile;
            }
            login::Clientbound::Disconnect(p) => {
                return Ok(Reason::component(&p.reason_json, lang));
            }
            login::Clientbound::Hello(hello) => {
                status(send, &tr(lang, "connect.authorizing"));
                let key = RsaPublicKey::from_der(&hello.public_key)
                    .map_err(|err| Reason::plain(format!("Protocol error: {err}")))?;
                let secret = generate_secret().map_err(e)?;
                if hello.should_authenticate {
                    let signed_in = auth.is_some();
                    let authenticated = auth
                        .ok_or_else(|| Reason::keyed(
                            "horizon.multiplayer.not_signed_in",
                            lang,
                        ))
                        .and_then(|account| {
                            let hash = server_hash(
                                &hello.server_id,
                                &secret,
                                &hello.public_key,
                            );
                            crate::auth::join_server(account, &hash)
                                .map_err(|err| {
                                    let (reason, key) = match err.reason() {
                                        Ok(key) => (tr(lang, key), Some(key.to_owned())),
                                        Err(message) => (message.to_owned(), None),
                                    };
                                    Reason {
                                        text: plain(
                                            &translatable("disconnect.loginFailedInfo", &[&reason]),
                                            lang,
                                        ),
                                        key,
                                    }
                                })
                        });
                    if let Err(reason) = authenticated {
                        if !continues_after_failed_join(lan, signed_in) {
                            return Err(reason);
                        }
                        log::warn!("Authentication failed: {}", reason.text);
                    }
                }
                status(send, &tr(lang, "connect.encrypting"));
                c.send(
                        &login::Serverbound::Key(login::Key {
                                key: key.encrypt(&secret).map_err(e)?,
                                challenge: key.encrypt(&hello.challenge).map_err(e)?,
                            })
                            .encode(),
                    )
                    .map_err(e)?;
                c.enable_encryption(&secret);
            }
            login::Clientbound::CustomQuery(q) => {
                c.send(
                        &login::Serverbound::CustomQueryAnswer(login::CustomQueryAnswer {
                                transaction: q.transaction,
                                payload: None,
                            })
                            .encode(),
                    )
                    .map_err(e)?;
            }
            login::Clientbound::CookieRequest(q) => {
                let payload = cookies.lock().unwrap().get(&q.key).cloned();
                c.send(
                        &login::Serverbound::CookieResponse(login::CookieResponse {
                                key: q.key,
                                payload,
                            })
                            .encode(),
                    )
                    .map_err(e)?;
            }
            login::Clientbound::Unknown(..) => {}
        }
    };
    status(send, &tr(lang, "connect.joining"));
    c.send(
            &cfg::Serverbound::CustomPayload(cfg::CustomPayload {
                    channel: "minecraft:brand".into(),
                    data: brand(),
                })
                .encode(),
        )
        .map_err(e)?;
    c.send(&cfg::Serverbound::ClientInformation(info).encode()).map_err(e)?;
    let (mut reader, writer) = c.split();
    *shared_writer.lock().unwrap() = Some(writer);
    let registries = configure(
        &mut reader,
        shared_writer,
        configuring,
        lang,
        send,
        cookies,
        resources,
    )?;
    let _ = send
        .send(Event::Joined {
            registries: Arc::new(registries),
            uuid: profile.uuid,
            name: profile.name,
        });
    receive_play(
        &mut reader,
        shared_writer,
        configuring,
        lang,
        send,
        cookies,
        far,
        resources,
        recorder,
    )
}
fn receive_play(
    reader: &mut ConnReader,
    shared_writer: &Mutex<Option<ConnWriter>>,
    configuring: &AtomicBool,
    lang: &Language,
    send: &Sender<Event>,
    cookies: &Cookies,
    far: &crate::lod_net::Inbox,
    resources: &Path,
    recorder: &RecorderSlot,
) -> Result<Reason, Reason> {
    loop {
        let (id, payload) = reader
            .recv()
            .map_err(|e| Reason::plain(connection_lost(e, lang)))?;
        let packet = match play::Clientbound::decode(id, &payload) {
            Ok(p) => p,
            Err(err) => {
                let name = horizon_protocol::ids::name(
                        horizon_protocol::State::Play,
                        horizon_protocol::Direction::Clientbound,
                        id,
                    )
                    .unwrap_or("?");
                let _ = send.send(Event::DecodeError(format!("{name}: {err}")));
                continue;
            }
        };
        if let Some(rec) = recorder.lock().unwrap().as_ref() && rec.is_active() {
            rec.received(&packet, id, &payload);
        }
        let reply = |p: play::Serverbound| {
            if let Some(w) = shared_writer.lock().unwrap().as_mut() {
                let _ = w.send(&p.encode());
            }
        };
        match packet {
            play::Clientbound::KeepAlive(k) => reply(play::Serverbound::KeepAlive(k)),
            play::Clientbound::BundleDelimiter(_) => {}
            play::Clientbound::ResourcePackPush(p) => {
                let _ = send.send(Event::PackPush(p));
            }
            play::Clientbound::ResourcePackPop(p) => {
                let _ = send.send(Event::PackPop(p.id));
            }
            play::Clientbound::StartConfiguration(_) => {
                let _ = send.send(Event::Reconfiguring);
                if let Some(w) = shared_writer.lock().unwrap().as_mut() {
                    let _ = w
                        .send(
                            &play::Serverbound::ConfigurationAcknowledged(Empty).encode(),
                        );
                    configuring.store(true, Ordering::Relaxed);
                }
                let registries = configure(
                    reader,
                    shared_writer,
                    configuring,
                    lang,
                    send,
                    cookies,
                    resources,
                )?;
                let _ = send.send(Event::Reconfigured(Arc::new(registries)));
            }
            play::Clientbound::CookieRequest(q) => {
                let payload = cookies.lock().unwrap().get(&q.key).cloned();
                reply(
                    play::Serverbound::CookieResponse(cfg::CookieResponse {
                        key: q.key,
                        payload,
                    }),
                );
            }
            play::Clientbound::StoreCookie(p) => {
                cookies.lock().unwrap().insert(p.key, p.payload);
            }
            play::Clientbound::ServerLinks(p) => {
                let _ = send.send(Event::ServerLinks(p.links));
            }
            play::Clientbound::Transfer(p) => {
                let _ = send
                    .send(Event::Transfer {
                        host: p.host,
                        port: p.port,
                    });
                return Ok(Reason::keyed("disconnect.transfer", lang));
            }
            play::Clientbound::Disconnect(d) => return Ok(Reason::tag(&d.reason, lang)),
            play::Clientbound::CustomPayload(p) if p.channel == "minecraft:brand" => {
                receive_brand(&p, send);
            }
            play::Clientbound::CustomPayload(
                p,
            ) if p.channel == crate::lod_net::CHANNEL => {
                far.deliver(p.data);
            }
            other => {
                if send.send(Event::Packet(Box::new(other))).is_err() {
                    return Ok(Reason::plain("Disconnected"));
                }
            }
        }
    }
}
/// The configuration phase: registries, tags and known packs until the
/// server finishes it (ClientConfigurationPacketListenerImpl).
fn configure(
    reader: &mut ConnReader,
    writer: &Mutex<Option<ConnWriter>>,
    configuring: &AtomicBool,
    lang: &Language,
    send: &Sender<Event>,
    cookies: &Cookies,
    resources: &Path,
) -> Result<Dynamic, Reason> {
    let mut registries = Dynamic::default();
    let mut content_verified = false;
    let reply = |p: cfg::Serverbound| -> Result<(), Reason> {
        match writer.lock().unwrap().as_mut() {
            Some(w) => {
                w.send(&p.encode()).map_err(|e| Reason::plain(connection_lost(e, lang)))
            }
            None => Err(Reason::plain("Disconnected")),
        }
    };
    loop {
        let (id, payload) = reader
            .recv()
            .map_err(|e| Reason::plain(connection_lost(e, lang)))?;
        match cfg::Clientbound::decode(id, &payload)
            .map_err(|e| Reason::plain(connection_lost(e, lang)))?
        {
            cfg::Clientbound::KeepAlive(k) => reply(cfg::Serverbound::KeepAlive(k))?,
            cfg::Clientbound::CustomPayload(
                p,
            ) if p.channel == crate::content_identity::CHANNEL => {
                let hash = crate::content_identity::identity(resources)
                    .map_err(|_| Reason::plain(
                        crate::content_identity::MISMATCH.to_string(),
                    ))?;
                content_verified = p.data == hash;
                reply(
                    cfg::Serverbound::CustomPayload(cfg::CustomPayload {
                        channel: crate::content_identity::CHANNEL.into(),
                        data: hash.to_vec(),
                    }),
                )?;
            }
            cfg::Clientbound::CustomPayload(
                p,
            ) if p.channel == horizon_protocol::mod_list::CHANNEL => {
                reply(
                    cfg::Serverbound::CustomPayload(cfg::CustomPayload {
                        channel: horizon_protocol::mod_list::CHANNEL.into(),
                        data: horizon_protocol::mod_list::encode(
                            &horizon_protocol::mod_list::shared(
                                &horizon_protocol::mod_list::local(),
                            ),
                        ),
                    }),
                )?;
            }
            cfg::Clientbound::CustomPayload(p) => {
                if p.channel == "minecraft:brand"
                    && horizon_protocol::buf::Reader::new(&p.data)
                        .string(32767)
                        .ok()
                        .as_deref() == Some("horizon")
                {
                    let mut w = horizon_protocol::buf::Writer::new();
                    w.string("horizon");
                    reply(
                        cfg::Serverbound::CustomPayload(cfg::CustomPayload {
                            channel: "minecraft:brand".into(),
                            data: w.data,
                        }),
                    )?;
                }
                receive_brand(&p, send);
            }
            cfg::Clientbound::Ping(p) => reply(cfg::Serverbound::Pong(p))?,
            cfg::Clientbound::SelectKnownPacks(p) => {
                reply(
                    cfg::Serverbound::SelectKnownPacks(cfg::SelectKnownPacks {
                        packs: select_known_packs(p.packs),
                    }),
                )?
            }
            cfg::Clientbound::UpdateEnabledFeatures(p) => {
                let _ = send.send(Event::Features(p.features));
            }
            cfg::Clientbound::RegistryData(mut r) => {
                fill_known_entries(&mut r, resources);
                registries.add(r);
            }
            cfg::Clientbound::UpdateTags(t) => registries.add_tags(&t),
            cfg::Clientbound::CodeOfConduct(c) => {
                let _ = send.send(Event::CodeOfConduct(c.text));
            }
            cfg::Clientbound::ResourcePackPush(p) => {
                let _ = send.send(Event::PackPush(p));
            }
            cfg::Clientbound::ResourcePackPop(p) => {
                let _ = send.send(Event::PackPop(p.id));
            }
            cfg::Clientbound::ShowDialog(d) => {
                let _ = send.send(Event::ShowDialog(d.dialog));
            }
            cfg::Clientbound::ClearDialog(_) => {
                let _ = send.send(Event::ClearDialog);
            }
            cfg::Clientbound::CookieRequest(q) => {
                let payload = cookies.lock().unwrap().get(&q.key).cloned();
                reply(
                    cfg::Serverbound::CookieResponse(cfg::CookieResponse {
                        key: q.key,
                        payload,
                    }),
                )?
            }
            cfg::Clientbound::StoreCookie(p) => {
                cookies.lock().unwrap().insert(p.key, p.payload);
            }
            cfg::Clientbound::ServerLinks(p) => {
                let _ = send.send(Event::ServerLinks(p.links));
            }
            cfg::Clientbound::Transfer(p) => {
                let _ = send
                    .send(Event::Transfer {
                        host: p.host,
                        port: p.port,
                    });
                return Err(Reason::keyed("disconnect.transfer", lang));
            }
            cfg::Clientbound::Disconnect(d) => return Err(Reason::tag(&d.reason, lang)),
            cfg::Clientbound::FinishConfiguration(_) => {
                registries.custom_components = content_verified;
                let mut w = writer.lock().unwrap_or_else(|e| e.into_inner());
                let w = w.as_mut().ok_or_else(|| Reason::plain("Disconnected"))?;
                w.send(&cfg::Serverbound::FinishConfiguration(Empty).encode())
                    .map_err(|e| Reason::plain(connection_lost(e, lang)))?;
                configuring.store(false, Ordering::Relaxed);
                return Ok(registries);
            }
            _ => {}
        }
    }
}
/// `KnownPacksManager.trySelectingPacks`: the packs of the server's list the
/// client has too. Horizon carries vanilla's core pack (the prepared
/// resources' `data/`), so it names that one when the server offers it, and
/// the server leaves that pack's entries' data out (`fill_known_entries`).
fn select_known_packs(offered: Vec<cfg::KnownPack>) -> Vec<cfg::KnownPack> {
    let (namespace, id, version) = crate::lan::data::CORE_PACK;
    offered
        .into_iter()
        .filter(|p| p.namespace == namespace && p.id == id && p.version == version)
        .collect()
}
fn valid_resource_segment(s: &str, allow_slash: bool) -> bool {
    !s.is_empty() && !s.starts_with('/') && !s.ends_with('/') && !s.contains("//")
        && s.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
        && s
            .bytes()
            .all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit()
                    || matches!(b, b'_' | b'-' | b'.') || (allow_slash && b == b'/')
            })
}
/// `RegistryDataCollector`: an entry of a pack both sides know comes without
/// its data, which the client reads from its own copy of the pack
/// (`data/<namespace>/<registry path>/<path>.json`, `Registries.elementsDirPath`).
fn fill_known_entries(r: &mut cfg::RegistryData, resources: &Path) {
    let (registry_ns, registry_path) = r
        .registry
        .split_once(':')
        .unwrap_or(("minecraft", r.registry.as_str()));
    if !valid_resource_segment(registry_ns, false)
        || !valid_resource_segment(registry_path, true)
    {
        return;
    }
    let dir = if registry_ns == "minecraft" {
        registry_path.to_owned()
    } else {
        format!("{registry_ns}/{registry_path}")
    };
    for entry in r.entries.iter_mut().filter(|e| e.data.is_none()) {
        let (ns, path) = entry
            .id
            .split_once(':')
            .unwrap_or(("minecraft", entry.id.as_str()));
        if !valid_resource_segment(ns, false) || !valid_resource_segment(path, true) {
            continue;
        }
        let file = resources
            .join("data")
            .join(ns)
            .join(&dir)
            .join(format!("{path}.json"));
        let json = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
        if let Some(json) = json {
            entry.data = Some(crate::lan::data::json_to_nbt(&json));
        }
    }
}
fn receive_brand(payload: &cfg::CustomPayload, send: &Sender<Event>) {
    {
        fn sunset_server_scan_config_channel(
            payload: &horizon_protocol::packets::configuration::CustomPayload,
        ) {
            if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
                return;
            }
            crate::net::sunset_packets::scan_config(payload);
        }
        sunset_server_scan_config_channel(payload)
    };
    if payload.channel == "minecraft:brand"
        && let Ok(brand) = horizon_protocol::buf::Reader::new(&payload.data)
            .string(32767)
    {
        let _ = send.send(Event::Brand(brand));
    } else if payload.channel == horizon_protocol::quiet::CHANNEL {
        let _ = send.send(Event::QuietCommands);
    } else if payload.channel == horizon_protocol::mod_net::CHANNEL {
        let _ = send.send(Event::ModNet);
    } else if payload.channel == crate::lod_net::CHANNEL
        && let Some(offer) = crate::lod_net::Offer::decode(&payload.data)
    {
        let _ = send.send(Event::FarTerrain(offer));
    }
}
/// `ClientBrandRetriever.getClientModName`: vanilla's default client brand.
fn brand() -> Vec<u8> {
    if let ::core::ops::ControlFlow::Break(__horizon_mixins_return) = {
        fn sunset_client_brand() -> std::ops::ControlFlow<Vec<u8>> {
            if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
                return std::ops::ControlFlow::Continue(());
            }
            match crate::net::sunset_client_brand::payload() {
                Some(data) => std::ops::ControlFlow::Break(data),
                None => std::ops::ControlFlow::Continue(()),
            }
        }
        sunset_client_brand()
    } {
        return __horizon_mixins_return;
    }
    let mut w = horizon_protocol::buf::Writer::new();
    w.string("vanilla");
    w.data
}
/// Flatten a JSON chat component (login disconnects are JSON).
/// At most `horizon_protocol::text::MAX_TEXT` bytes, as `plain`: a login-phase reason from a
/// server is any JSON it likes.
pub fn json_text(v: &serde_json::Value, lang: &Language) -> String {
    use horizon_protocol::text::{MAX_TEXT, push_capped};
    match v {
        serde_json::Value::String(s) => {
            let mut out = String::new();
            push_capped(&mut out, s);
            out
        }
        serde_json::Value::Array(a) => {
            let mut out = String::new();
            for e in a {
                if out.len() >= MAX_TEXT {
                    break;
                }
                push_capped(&mut out, &json_text(e, lang));
            }
            out
        }
        serde_json::Value::Object(o) => {
            let mut out = if let Some(t) = o.get("text").and_then(|t| t.as_str()) {
                let mut out = String::new();
                push_capped(&mut out, t);
                out
            } else if let Some(key) = o.get("translate").and_then(|t| t.as_str()) {
                let args: Vec<String> = o
                    .get("with")
                    .and_then(|w| w.as_array())
                    .map(|a| a.iter().map(|e| json_text(e, lang)).collect())
                    .unwrap_or_default();
                let pattern = lang
                    .get(key)
                    .cloned()
                    .or_else(|| {
                        o.get("fallback").and_then(|f| f.as_str()).map(str::to_owned)
                    })
                    .unwrap_or_else(|| key.to_owned());
                horizon_protocol::text::format_java(&pattern, &args)
            } else {
                String::new()
            };
            if let Some(extra) = o.get("extra").and_then(|e| e.as_array()) {
                for e in extra {
                    if out.len() >= MAX_TEXT {
                        break;
                    }
                    push_capped(&mut out, &json_text(e, lang));
                }
            }
            out
        }
        other => other.to_string(),
    }
}
/// The language a session resolves server-sent components with: the selected
/// one as `lang` loaded it (`en_us` under it, packs included), so a menu title
/// the server sends as a key follows the language screen. A process with no
/// language set (a dedicated server's log) reads en_us.json.
pub fn load_language(resources: &std::path::Path) -> Language {
    if let Some(lang) = crate::lang::current() {
        return lang.map().clone();
    }
    horizon_client_core::lang::Language::load(&[&resources.to_path_buf()], "en_us")
        .map()
        .clone()
}
/// Ask a server for its status (ServerStatusPinger): MOTD, players and the
/// round trip time in milliseconds.
pub fn ping(
    address: &str,
    timeout: Duration,
) -> Result<(serde_json::Value, u128), String> {
    use horizon_protocol::packets::status;
    let (host, port) = parse_address(address).ok_or("Invalid address")?;
    let mut c = Connection::connect_server(&host, port, timeout)
        .map_err(|e| e.to_string())?;
    c.reader.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
    let e = |e: horizon_protocol::Error| e.to_string();
    c.send(
            &handshake::Serverbound::Intention(handshake::Intention {
                    protocol: PROTOCOL_VERSION,
                    host: host.clone(),
                    port,
                    intent: handshake::STATUS,
                })
                .encode(),
        )
        .map_err(e)?;
    c.send(&status::Serverbound::StatusRequest(Empty).encode()).map_err(e)?;
    let (id, payload) = c.recv().map_err(e)?;
    let json = match status::Clientbound::decode(id, &payload).map_err(e)? {
        status::Clientbound::StatusResponse(s) => {
            serde_json::from_str(&s.json).map_err(|e| e.to_string())?
        }
        _ => return Err("unexpected status reply".into()),
    };
    let start = std::time::Instant::now();
    c.send(&status::Serverbound::PingRequest(status::Ping { time: 1 }).encode())
        .map_err(e)?;
    let _ = c.recv().map_err(e)?;
    Ok((json, start.elapsed().as_millis()))
}
/// What a server that answers only the legacy ping reports
/// (`LegacyServerPinger.Output.handleResponse`).
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyStatus {
    pub protocol: i32,
    pub version: String,
    pub motd: String,
    pub players: i32,
    pub max_players: i32,
}
/// `Mth.getInt(str, default)`.
fn legacy_int(s: &str, default: i32) -> i32 {
    s.parse().unwrap_or(default)
}
/// `LegacyServerPinger.channelRead0` on the string after the 0xFF byte: split
/// on NUL (at most 6 parts) and the first part must be `§1`.
fn parse_legacy_status(text: &str) -> Option<LegacyStatus> {
    let parts: Vec<&str> = text.splitn(6, '\0').collect();
    if parts.first() != Some(&"\u{a7}1") || parts.len() < 6 {
        return None;
    }
    Some(LegacyStatus {
        protocol: legacy_int(parts[1], 0),
        version: parts[2].to_owned(),
        motd: parts[3].to_owned(),
        players: legacy_int(parts[4], -1),
        max_players: legacy_int(parts[5], -1),
    })
}
/// `LegacyProtocolUtils.writeLegacyString`: the UTF-16 length, then UTF-16BE.
fn write_legacy_string(out: &mut Vec<u8>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    out.extend_from_slice(&(units.len() as u16).to_be_bytes());
    for unit in units {
        out.extend_from_slice(&unit.to_be_bytes());
    }
}
/// `LegacyServerPinger.channelActive`: the 1.6 `0xFE 0x01 0xFA MC|PingHost` request.
fn legacy_ping_request(host: &str, port: u16) -> Vec<u8> {
    let mut out = vec![254, 1, 250];
    write_legacy_string(&mut out, "MC|PingHost");
    let mut payload = vec![127];
    write_legacy_string(&mut payload, host);
    payload.extend_from_slice(&i32::from(port).to_be_bytes());
    out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    out.extend_from_slice(&payload);
    out
}
/// `ServerStatusPinger.pingLegacyServer`: ask a server that does not answer
/// the status handshake with the pre-1.7 ping.
pub fn ping_legacy(address: &str, timeout: Duration) -> Result<LegacyStatus, String> {
    let (host, port) = parse_address(address).ok_or("Invalid address")?;
    let mut c = Connection::connect_server(&host, port, timeout)
        .map_err(|e| e.to_string())?;
    c.reader.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
    let e = |e: horizon_protocol::Error| e.to_string();
    c.writer.write_raw(&legacy_ping_request(&host, port)).map_err(e)?;
    let head = c.reader.read_raw(3).map_err(e)?;
    if head[0] != 255 {
        return Err("unexpected legacy reply".into());
    }
    let units = usize::from(u16::from_be_bytes([head[1], head[2]]));
    let bytes = c.reader.read_raw(units * 2).map_err(e)?;
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u16::from_be_bytes(*p))
        .collect();
    parse_legacy_status(&String::from_utf16_lossy(&units))
        .ok_or_else(|| "unexpected legacy reply".to_owned())
}
/// `ClientHandshakePacketListenerImpl.handleHello`: a failed session join goes
/// on to encryption only for an entry picked from LAN discovery
/// (`ServerData.isLan`), and the host decides. A vanilla client always has an
/// account; Horizon without one never joins anything (Mojang's rules: no play
/// without an account that owns the game), LAN included.
fn continues_after_failed_join(lan: bool, signed_in: bool) -> bool {
    lan && signed_in
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_login_reason_repeating_its_argument_stays_bounded() {
        let pattern = "%1$s".repeat(16_000);
        let mut v = serde_json::json!({ "text" : "A" });
        for _ in 0..3 {
            v = serde_json::json!(
                { "translate" : "x.unknown", "fallback" : pattern, "with" : [v] }
            );
        }
        let v = serde_json::json!({ "text" : "", "extra" : [v.clone(), v] });
        let text = json_text(&v, &Language::default());
        assert!(text.len() <= horizon_protocol::text::MAX_TEXT);
    }
    fn pack(namespace: &str, id: &str, version: &str) -> cfg::KnownPack {
        cfg::KnownPack {
            namespace: namespace.into(),
            id: id.into(),
            version: version.into(),
        }
    }
    #[test]
    fn the_client_knows_vanilla_core_pack_of_its_own_version_only() {
        let core = pack("minecraft", "core", horizon_protocol::GAME_VERSION);
        let offered = vec![
            core.clone(), pack("minecraft", "trade_rebalance",
            horizon_protocol::GAME_VERSION,), pack("example", "server_pack", "1"),
        ];
        assert_eq!(select_known_packs(offered), vec![core]);
        assert_eq!(
            select_known_packs(vec![pack("minecraft", "core", "1.21.11")]), vec![]
        );
        assert_eq!(select_known_packs(vec![]), vec![]);
    }
    #[test]
    fn a_known_entry_takes_its_data_from_the_clients_pack() {
        let root = std::env::temp_dir()
            .join(format!("hz-known-entries-{}", std::process::id()));
        let dir = root.join("data/minecraft/damage_type");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
                dir.join("in_fire.json"),
                br#"{"message_id": "inFire", "exhaustion": 0.1, "scaling": "when_caused_by_living_non_player"}"#,
            )
            .unwrap();
        let sent = Tag::compound(
            vec![("message_id", Tag::String(JavaString::from("custom")),)],
        );
        let mut r = cfg::RegistryData {
            registry: "minecraft:damage_type".into(),
            entries: vec![
                cfg::RegistryEntry { id : "minecraft:in_fire".into(), data : None, },
                cfg::RegistryEntry { id : "minecraft:custom".into(), data : Some(sent
                .clone()), }, cfg::RegistryEntry { id : "minecraft:missing".into(), data
                : None, },
            ],
        };
        fill_known_entries(&mut r, &root);
        std::fs::remove_dir_all(&root).unwrap();
        let Some(Tag::Compound(filled)) = &r.entries[0].data else {
            panic!("in_fire was not filled: {:?}", r.entries[0].data);
        };
        assert_eq!(
            filled.get(& JavaString::from("message_id")), Some(&
            Tag::String(JavaString::from("inFire")))
        );
        assert_eq!(r.entries[1].data, Some(sent));
        assert_eq!(r.entries[2].data, None);
    }
    /// Launchers find the last server joined in `latest.log` by vanilla's line.
    #[test]
    fn the_connecting_line_is_vanillas() {
        let line = |address: &str| {
            let (host, port) = parse_address(address).unwrap();
            connecting_line(&host, port)
        };
        assert_eq!(line("play.example.net"), "Connecting to play.example.net, 25565");
        assert_eq!(
            line("play.example.net:25577"), "Connecting to play.example.net, 25577"
        );
        assert_eq!(line("[::1]:25566"), "Connecting to ::1, 25566");
    }
    #[test]
    fn direct_login_preserves_hostname_and_negotiates_brand() {
        for server_brand in [
            b"\x07vanilla".as_slice(),
            b"\x07horizon",
            b"\x05Paper",
            b"\xff",
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let session = Session::connect(
                &format!("localhost:{port}"),
                "Probe",
                None,
                cfg::ClientInformation::new(8),
                Arc::new(Language::new()),
                PathBuf::new(),
            );
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut server = Connection::from_stream(stream).unwrap();
            let (id, body) = server.recv().unwrap();
            let handshake::Serverbound::Intention(packet) = handshake::Serverbound::decode(
                    id,
                    &body,
                )
                .unwrap() else { panic!("expected login handshake") };
            assert_eq!((packet.host.as_str(), packet.port), ("localhost", port));
            assert_eq!(packet.intent, handshake::LOGIN);
            server.recv().unwrap();
            server
                .send(
                    &login::Clientbound::LoginFinished(login::LoginFinished {
                            profile: login::GameProfile {
                                uuid: 1,
                                name: "Probe".into(),
                                properties: vec![],
                            },
                            session: 1,
                        })
                        .encode(),
                )
                .unwrap();
            let (id, body) = server.recv().unwrap();
            assert!(
                matches!(login::Serverbound::decode(id, & body).unwrap(),
                login::Serverbound::LoginAcknowledged(_))
            );
            let (id, body) = server.recv().unwrap();
            assert_eq!(
                cfg::Serverbound::decode(id, & body).unwrap(),
                cfg::Serverbound::CustomPayload(cfg::CustomPayload { channel :
                "minecraft:brand".into(), data : b"\x07vanilla".to_vec(), })
            );
            let (id, body) = server.recv().unwrap();
            assert!(
                matches!(cfg::Serverbound::decode(id, & body).unwrap(),
                cfg::Serverbound::ClientInformation(_))
            );
            server
                .send(
                    &cfg::Clientbound::CustomPayload(cfg::CustomPayload {
                            channel: "minecraft:brand".into(),
                            data: server_brand.to_vec(),
                        })
                        .encode(),
                )
                .unwrap();
            server
                .send(
                    &cfg::Clientbound::SelectKnownPacks(cfg::SelectKnownPacks {
                            packs: vec![],
                        })
                        .encode(),
                )
                .unwrap();
            if server_brand == b"\x07horizon" {
                let (id, body) = server.recv().unwrap();
                assert_eq!(
                    cfg::Serverbound::decode(id, & body).unwrap(),
                    cfg::Serverbound::CustomPayload(cfg::CustomPayload { channel :
                    "minecraft:brand".into(), data : b"\x07horizon".to_vec(), })
                );
            }
            let (id, body) = server.recv().unwrap();
            assert_eq!(
                cfg::Serverbound::decode(id, & body).unwrap(),
                cfg::Serverbound::SelectKnownPacks(cfg::SelectKnownPacks { packs : vec![]
                })
            );
            drop(server);
            drop(session);
        }
    }
    #[test]
    fn legacy_ping_request_is_the_1_6_ping_host_packet() {
        let bytes = legacy_ping_request("a.b", 25565);
        assert_eq!(& bytes[..3], & [254, 1, 250]);
        assert_eq!(& bytes[3..5], & [0, 11]);
        let after = 5 + 22;
        assert_eq!(& bytes[after..after + 2], & [0, 1 + 2 + 6 + 4]);
        assert_eq!(bytes[after + 2], 127);
        assert_eq!(& bytes[after + 3..after + 5], & [0, 3]);
        assert_eq!(& bytes[bytes.len() - 4..], & 25565i32.to_be_bytes());
    }
    #[test]
    fn legacy_status_is_split_on_nul_after_the_section_one_marker() {
        let status = parse_legacy_status(
                "\u{a7}1\x00127\x001.6.4\x00A \u{a7}aserver\x003\x0020",
            )
            .unwrap();
        assert_eq!(
            status, LegacyStatus { protocol : 127, version : "1.6.4".into(), motd :
            "A \u{a7}aserver".into(), players : 3, max_players : 20, }
        );
        let status = parse_legacy_status("\u{a7}1\x00x\x00v\x00m\x00?\x00?").unwrap();
        assert_eq!((status.protocol, status.players, status.max_players), (0, - 1, - 1));
        assert!(parse_legacy_status("kick\x00127\x00v\x00m\x001\x002").is_none());
    }
    /// The en_us text of the disconnect reasons.
    fn disconnect_language() -> Language {
        [
            ("disconnect.timeout", "Timed out"),
            ("disconnect.endOfStream", "End of stream"),
            ("disconnect.genericReason", "%s"),
        ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect()
    }
    #[test]
    fn a_server_that_stops_answering_times_out() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let held = std::thread::spawn(move || {
            listener.accept().map(|(stream, _)| stream)
        });
        let (c, _, _) = connect_tcp("127.0.0.1", port, true, Duration::from_millis(300))
            .unwrap();
        let _server = held.join().unwrap().unwrap();
        let (mut reader, writer) = c.split();
        let (done, result) = channel();
        std::thread::spawn(move || {
            let (send, _events) = channel();
            let _ = done
                .send(
                    receive_play(
                        &mut reader,
                        &Mutex::new(Some(writer)),
                        &AtomicBool::new(false),
                        &disconnect_language(),
                        &send,
                        &Cookies::default(),
                        &crate::lod_net::Inbox::default(),
                        Path::new("."),
                        &Arc::new(Mutex::new(None)),
                    ),
                );
        });
        let ended = result
            .recv_timeout(Duration::from_secs(10))
            .expect("the connection never timed out");
        assert_eq!(
            ended.map(| r | r.text).map_err(| r | r.text), Err("Timed out".to_owned())
        );
    }
    #[test]
    fn lost_connections_read_like_vanillas_disconnect_reasons() {
        let lang = disconnect_language();
        let lost = |e| connection_lost(e, &lang);
        assert_eq!(lost(horizon_protocol::Error::timed_out()), "Timed out");
        assert_eq!(lost(horizon_protocol::Error::end_of_stream()), "End of stream");
        assert_eq!(
            lost(horizon_protocol::Error::new("Bad VarInt")),
            "Internal Exception: Bad VarInt"
        );
    }
    #[test]
    fn configuration_answers_the_content_identity_challenge() {
        let root = std::env::temp_dir()
            .join(format!("horizon-content-identity-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
                root.join("facts.json"),
                br#"{"states":[["minecraft:stone",false,false,"Block{minecraft:stone}",{"id":"minecraft:stone"}]]}"#,
            )
            .unwrap();
        let expected = crate::content_identity::identity(&root).unwrap();
        let (client, server) = horizon_protocol::connection::Pipe::pair();
        let (mut reader, writer) = Connection::local(client).split();
        let writer = Mutex::new(Some(writer));
        let thread = std::thread::spawn(move || {
            let (send, _events) = channel();
            configure(
                &mut reader,
                &writer,
                &AtomicBool::new(true),
                &Language::new(),
                &send,
                &Cookies::default(),
                &root,
            )
        });
        let mut server = Connection::local(server);
        server
            .send(
                &cfg::Clientbound::CustomPayload(cfg::CustomPayload {
                        channel: crate::content_identity::CHANNEL.into(),
                        data: vec![],
                    })
                    .encode(),
            )
            .unwrap();
        server.send(&cfg::Clientbound::Ping(cfg::Ping { id: 17 }).encode()).unwrap();
        let (id, body) = server.recv().unwrap();
        assert_eq!(
            cfg::Serverbound::decode(id, & body).unwrap(),
            cfg::Serverbound::CustomPayload(cfg::CustomPayload { channel : crate
            ::content_identity::CHANNEL.into(), data : expected.to_vec(), })
        );
        let (id, body) = server.recv().unwrap();
        assert_eq!(
            cfg::Serverbound::decode(id, & body).unwrap(),
            cfg::Serverbound::Pong(cfg::Ping { id : 17 })
        );
        server.send(&cfg::Clientbound::FinishConfiguration(Empty).encode()).unwrap();
        let (id, body) = server.recv().unwrap();
        assert_eq!(
            cfg::Serverbound::decode(id, & body).unwrap(),
            cfg::Serverbound::FinishConfiguration(Empty)
        );
        assert!(thread.join().unwrap().is_ok());
        std::fs::remove_dir_all(
                std::env::temp_dir()
                    .join(format!("horizon-content-identity-{}", std::process::id())),
            )
            .unwrap();
    }
    #[test]
    fn disconnect_and_drop_do_not_wait_for_a_stalled_reader() {
        for explicit_disconnect in [true, false] {
            let (client, _server) = horizon_protocol::connection::Pipe::pair();
            let (_reader, writer) = Connection::local(client).split();
            let writer = Arc::new(Mutex::new(Some(writer)));
            let (_send, events) = channel();
            let (release, stalled) = channel();
            let (finished, completed) = channel();
            let thread = std::thread::spawn(move || {
                let _ = stalled.recv_timeout(std::time::Duration::from_secs(3));
                let _ = finished.send(());
            });
            let mut session = Session {
                events,
                writer: writer.clone(),
                configuring: Arc::new(AtomicBool::new(false)),
                thread: Some(thread),
                address: "local".into(),
                name: "TestPlayer".into(),
                counters: Arc::new(Mutex::new(None)),
                cookies: Cookies::default(),
                debug: false,
                far: Arc::default(),
                recorder: Arc::new(Mutex::new(None)),
                serial: next_serial(),
            };
            let started = std::time::Instant::now();
            if explicit_disconnect {
                session.disconnect();
                session.disconnect();
            }
            drop(session);
            let elapsed = started.elapsed();
            let reader_still_pending = completed.try_recv().is_err();
            let _ = release.send(());
            if reader_still_pending {
                completed.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
            }
            assert!(writer.lock().unwrap().is_none(), "connection was not closed");
            assert!(reader_still_pending, "disconnect waited for the stalled reader");
            assert!(
                elapsed < std::time::Duration::from_secs(1),
                "disconnect took {elapsed:?}"
            );
        }
    }
    #[test]
    fn pings_wait_behind_teleports_but_keepalives_do_not_wait_for_the_game() {
        let (client, server) = horizon_protocol::connection::Pipe::pair();
        let (mut reader, writer) = Connection::local(client).split();
        let writer = Arc::new(Mutex::new(Some(writer)));
        let (send, events) = channel();
        let thread_writer = writer.clone();
        let thread = std::thread::spawn(move || {
            receive_play(
                &mut reader,
                &thread_writer,
                &AtomicBool::new(false),
                &Language::new(),
                &send,
                &Cookies::default(),
                &crate::lod_net::Inbox::default(),
                Path::new("."),
                &Arc::new(Mutex::new(None)),
            )
        });
        let mut server = Connection::local(server);
        let teleport = play::Clientbound::PlayerPosition(play::PlayerPosition {
            teleport: 42,
            position: [10.0, 64.0, 10.0],
            delta: [0.0; 3],
            yaw: 90.0,
            pitch: 0.0,
            relatives: 0,
        });
        server.send(&teleport.encode()).unwrap();
        server.send(&play::Clientbound::Ping(cfg::Ping { id: 7 }).encode()).unwrap();
        server
            .send(&play::Clientbound::KeepAlive(cfg::KeepAlive { id: 9 }).encode())
            .unwrap();
        let (id, body) = server.recv().unwrap();
        assert_eq!(
            play::Serverbound::decode(id, & body).unwrap(),
            play::Serverbound::KeepAlive(cfg::KeepAlive { id : 9 })
        );
        let timeout = Duration::from_secs(5);
        assert!(
            matches!(events.recv_timeout(timeout).unwrap(), Event::Packet(p) if * p ==
            teleport)
        );
        assert!(
            matches!(events.recv_timeout(timeout).unwrap(), Event::Packet(p) if * p ==
            play::Clientbound::Ping(cfg::Ping { id : 7 }))
        );
        writer.lock().unwrap().take().unwrap().shutdown();
        assert!(thread.join().unwrap().is_err());
    }
    #[test]
    fn play_packets_wait_out_a_reconfiguration() {
        let (client, server) = horizon_protocol::connection::Pipe::pair();
        let (mut reader, writer) = Connection::local(client).split();
        let (send, events) = channel();
        let session = Session {
            events,
            writer: Arc::new(Mutex::new(Some(writer))),
            configuring: Arc::new(AtomicBool::new(false)),
            thread: None,
            address: "local".into(),
            name: "Player".into(),
            counters: Arc::new(Mutex::new(None)),
            cookies: Cookies::default(),
            debug: false,
            far: Arc::default(),
            recorder: Arc::new(Mutex::new(None)),
            serial: next_serial(),
        };
        let (thread_writer, thread_configuring, thread_recorder) = (
            session.writer.clone(),
            session.configuring.clone(),
            session.recorder.clone(),
        );
        let thread = std::thread::spawn(move || {
            receive_play(
                &mut reader,
                &thread_writer,
                &thread_configuring,
                &Language::new(),
                &send,
                &Cookies::default(),
                &crate::lod_net::Inbox::default(),
                Path::new("."),
                &thread_recorder,
            )
        });
        let mut server = Connection::local(server);
        let tick_end = play::Serverbound::ClientTickEnd(Empty);
        server.send(&play::Clientbound::StartConfiguration(Empty).encode()).unwrap();
        let (id, body) = server.recv().unwrap();
        assert_eq!(
            play::Serverbound::decode(id, & body).unwrap(),
            play::Serverbound::ConfigurationAcknowledged(Empty)
        );
        session.send(tick_end.clone());
        server.send(&cfg::Clientbound::FinishConfiguration(Empty).encode()).unwrap();
        let (id, body) = server.recv().unwrap();
        assert_eq!(
            cfg::Serverbound::decode(id, & body).unwrap(),
            cfg::Serverbound::FinishConfiguration(Empty)
        );
        session.send(tick_end.clone());
        let (id, body) = server.recv().unwrap();
        assert_eq!(play::Serverbound::decode(id, & body).unwrap(), tick_end);
        drop(session);
        assert!(thread.join().unwrap().is_err());
    }
    #[test]
    fn play_answers_cookie_requests_from_stored_cookies_and_follows_transfers() {
        let (client, server) = horizon_protocol::connection::Pipe::pair();
        let (mut reader, writer) = Connection::local(client).split();
        let writer = Arc::new(Mutex::new(Some(writer)));
        let (send, events) = channel();
        let cookies = Cookies::default();
        let (thread_writer, thread_cookies) = (writer.clone(), cookies.clone());
        let thread = std::thread::spawn(move || {
            receive_play(
                &mut reader,
                &thread_writer,
                &AtomicBool::new(false),
                &Language::new(),
                &send,
                &thread_cookies,
                &crate::lod_net::Inbox::default(),
                Path::new("."),
                &Arc::new(Mutex::new(None)),
            )
        });
        let mut server = Connection::local(server);
        let ask = |server: &mut Connection, key: &str| {
            let request = play::Clientbound::CookieRequest(cfg::CookieRequest {
                key: key.into(),
            });
            server.send(&request.encode()).unwrap();
            let (id, body) = server.recv().unwrap();
            play::Serverbound::decode(id, &body).unwrap()
        };
        assert_eq!(
            ask(& mut server, "proxy:session"),
            play::Serverbound::CookieResponse(cfg::CookieResponse { key : "proxy:session"
            .into(), payload : None, })
        );
        let store = play::Clientbound::StoreCookie(cfg::StoreCookie {
            key: "proxy:session".into(),
            payload: vec![1, 2, 3],
        });
        server.send(&store.encode()).unwrap();
        assert_eq!(
            ask(& mut server, "proxy:session"),
            play::Serverbound::CookieResponse(cfg::CookieResponse { key : "proxy:session"
            .into(), payload : Some(vec![1, 2, 3]), })
        );
        let transfer = play::Clientbound::Transfer(cfg::Transfer {
            host: "example.net".into(),
            port: 25566,
        });
        server.send(&transfer.encode()).unwrap();
        assert!(thread.join().unwrap().is_ok());
        assert!(
            matches!(events.try_iter().last(), Some(Event::Transfer { host, port : 25566
            }) if host == "example.net")
        );
        assert_eq!(cookies.lock().unwrap().get("proxy:session"), Some(& vec![1, 2, 3]));
    }
    #[test]
    fn server_brand_payload_is_decoded_and_invalid_or_unrelated_payloads_are_ignored() {
        let (send, events) = channel();
        let mut writer = horizon_protocol::buf::Writer::new();
        writer.string("vanilla");
        let mut payload = cfg::CustomPayload {
            channel: "minecraft:brand".into(),
            data: writer.data,
        };
        receive_brand(&payload, &send);
        assert!(
            matches!(events.try_recv(), Ok(Event::Brand(brand)) if brand == "vanilla")
        );
        payload.channel = "example:other".into();
        receive_brand(&payload, &send);
        assert!(events.try_recv().is_err());
        payload.channel = "minecraft:brand".into();
        payload.data = vec![0xff];
        receive_brand(&payload, &send);
        assert!(events.try_recv().is_err());
    }
    /// No account never joins; the signed-in rule is compared with the real
    /// client login listener by test_lan_auth_parity.py.
    #[test]
    fn lan_login_continues_after_authentication_failure() {
        for lan in [true, false] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let worker = std::thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
                let mut server = Connection::from_stream(stream).unwrap();
                server.recv().unwrap();
                server.recv().unwrap();
                let key = horizon_protocol::rsa::RsaPrivateKey::generate(1024).unwrap();
                let challenge = vec![1, 2, 3, 4];
                server
                    .send(
                        &login::Clientbound::Hello(login::Hello {
                                server_id: String::new(),
                                public_key: key.public_der(),
                                challenge: challenge.clone(),
                                should_authenticate: true,
                            })
                            .encode(),
                    )
                    .unwrap();
                let Ok((id, body)) = server.recv() else {
                    return false;
                };
                let login::Serverbound::Key(reply) = login::Serverbound::decode(
                        id,
                        &body,
                    )
                    .unwrap() else { panic!("client did not send encryption reply") };
                assert_eq!(key.decrypt(& reply.challenge).unwrap(), challenge);
                let secret: [u8; 16] = key
                    .decrypt(&reply.key)
                    .unwrap()
                    .try_into()
                    .unwrap();
                server.enable_encryption(&secret);
                server
                    .send(
                        &login::Clientbound::Disconnect(login::Disconnect {
                                reason_json: r#"{"text":"LAN fixture accepted"}"#.into(),
                            })
                            .encode(),
                    )
                    .unwrap();
                true
            });
            let address = format!("127.0.0.1:{port}");
            let connect = if lan { Session::connect_lan } else { Session::connect };
            let lang: Language = [
                (
                    "horizon.multiplayer.not_signed_in".to_owned(),
                    NOT_SIGNED_IN.to_owned(),
                ),
            ]
                .into_iter()
                .collect();
            let session = connect(
                &address,
                "Tester",
                None,
                cfg::ClientInformation::new(8),
                Arc::new(lang),
                PathBuf::new(),
            );
            let reason = loop {
                if let Event::Disconnected { reason, .. } = session
                    .events
                    .recv_timeout(Duration::from_secs(60))
                    .unwrap()
                {
                    break reason;
                }
            };
            let sent_key = worker.join().unwrap();
            assert!(! sent_key, "lan {lan}: joined without an account");
            assert_eq!(reason, NOT_SIGNED_IN);
            let goes_on = continues_after_failed_join(lan, true);
            assert_eq!(goes_on, lan);
            println!(
                "lan_auth={}:{}:{}", if lan { "LAN" } else { "OTHER" }, goes_on, !
                goes_on
            );
        }
    }
    /// A recorder that keeps what reaches it, as (id, payload).
    #[derive(Clone, Default)]
    struct TestRecorder(Arc<Mutex<Vec<(i32, Vec<u8>)>>>);
    impl PacketRecorder for TestRecorder {
        fn is_active(&self) -> bool {
            true
        }
        fn received(&self, _packet: &play::Clientbound, id: i32, payload: &[u8]) {
            self.0.lock().unwrap().push((id, payload.to_vec()));
        }
        fn record_packet(&self, id: i32, payload: &[u8]) {
            self.0.lock().unwrap().push((id, payload.to_vec()));
        }
        fn status(&self) -> RecorderStatus {
            RecorderStatus {
                active: true,
                path: PathBuf::from("test.mcpr"),
                elapsed: Duration::ZERO,
                packet_count: self.0.lock().unwrap().len() as u64,
                bytes_written: 0,
                error: None,
            }
        }
        fn stop(self: Box<Self>) -> std::io::Result<PathBuf> {
            Ok(PathBuf::from("test.mcpr"))
        }
    }
    impl TestRecorder {
        fn decoded(&self) -> Vec<play::Clientbound> {
            let packets = self.0.lock().unwrap();
            packets
                .iter()
                .map(|(id, payload)| play::Clientbound::decode(*id, payload).unwrap())
                .collect()
        }
    }
    #[test]
    fn session_recorder_gets_the_play_packets_the_network_thread_receives() {
        let (client, server) = horizon_protocol::connection::Pipe::pair();
        let (mut reader, writer) = Connection::local(client).split();
        let (send, events) = channel();
        let session = Session {
            events,
            writer: Arc::new(Mutex::new(Some(writer))),
            configuring: Arc::new(AtomicBool::new(false)),
            thread: None,
            address: "local".into(),
            name: "StudioTester".into(),
            counters: Arc::new(Mutex::new(None)),
            cookies: Cookies::default(),
            debug: false,
            far: Arc::default(),
            recorder: Arc::new(Mutex::new(None)),
            serial: next_serial(),
        };
        let recorder = TestRecorder::default();
        session.start_recording(recorder.clone(), |_| {});
        assert!(session.is_recording());
        let (thread_writer, thread_configuring, thread_recorder) = (
            session.writer.clone(),
            session.configuring.clone(),
            session.recorder.clone(),
        );
        let thread = std::thread::spawn(move || {
            receive_play(
                &mut reader,
                &thread_writer,
                &thread_configuring,
                &Language::new(),
                &send,
                &Cookies::default(),
                &crate::lod_net::Inbox::default(),
                Path::new("."),
                &thread_recorder,
            )
        });
        let mut server = Connection::local(server);
        let pos_pkt = play::Clientbound::PlayerPosition(play::PlayerPosition {
            teleport: 1,
            position: [100.0, 64.0, -200.0],
            delta: [0.0; 3],
            yaw: 45.0,
            pitch: 10.0,
            relatives: 0,
        });
        server.send(&pos_pkt.encode()).unwrap();
        let disc_pkt = play::Clientbound::Disconnect(cfg::Disconnect {
            reason: Tag::String("Replay tap test completed".into()),
        });
        server.send(&disc_pkt.encode()).unwrap();
        let _ = thread.join().unwrap();
        assert_eq!(
            session.stop_recording().unwrap().unwrap(), PathBuf::from("test.mcpr")
        );
        assert!(! session.is_recording());
        assert_eq!(recorder.decoded(), [pos_pkt, disc_pkt]);
    }
    #[test]
    fn session_recorder_gets_the_packets_the_client_makes() {
        use horizon_protocol::entity_data::{Value, serializer};
        let session = Session::replay("StudioTester");
        session
            .record_clientbound(
                &play::Clientbound::SwingAnimation(play::SwingAnimation {
                    id: 1,
                    hand: 0,
                    kind: 0,
                    duration: 6,
                }),
            );
        let recorder = TestRecorder::default();
        let snapshot = play::Clientbound::PlayerPosition(play::PlayerPosition {
            teleport: 0,
            position: [12.5, 64.0, -88.5],
            delta: [0.0; 3],
            yaw: 90.0,
            pitch: 15.0,
            relatives: 0,
        });
        session
            .start_recording(
                recorder.clone(),
                |rec| {
                    let encoded = snapshot.encode();
                    let mut reader = horizon_protocol::buf::Reader::new(&encoded);
                    let id = reader.varint().unwrap();
                    rec.record_packet(id, reader.rest());
                },
            );
        let swing_pkt = play::Clientbound::SwingAnimation(play::SwingAnimation {
            id: 42,
            hand: 0,
            kind: 1,
            duration: 6,
        });
        session.record_clientbound(&swing_pkt);
        let data_pkt = play::Clientbound::SetEntityData(play::SetEntityData {
            id: 42,
            values: vec![
                (0, serializer::BYTE, Value::Byte(0x02)), (6, serializer::POSE,
                Value::Id(5)),
            ],
        });
        session.record_clientbound(&data_pkt);
        assert_eq!(session.recorder_status().unwrap().packet_count, 3);
        session.stop_recording().unwrap().unwrap();
        session.record_clientbound(&swing_pkt);
        assert_eq!(recorder.decoded(), [snapshot, swing_pkt, data_pkt]);
    }
    #[test]
    fn fill_known_entries_rejects_path_traversal_and_reads_valid_entries() {
        let temp_dir = std::env::temp_dir()
            .join(format!("horizon-net-reg-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let resources = temp_dir.join("res");
        let valid_dir = resources.join("data/minecraft/worldgen/biome");
        std::fs::create_dir_all(&valid_dir).unwrap();
        std::fs::write(valid_dir.join("plains.json"), br#"{"has_precipitation":true}"#)
            .unwrap();
        std::fs::write(temp_dir.join("secret.json"), br#"{"leaked":true}"#).unwrap();
        let mut reg = cfg::RegistryData {
            registry: "minecraft:worldgen/biome".into(),
            entries: vec![
                cfg::RegistryEntry { id : "minecraft:plains".into(), data : None, },
                cfg::RegistryEntry { id : "minecraft:../../../../secret".into(), data :
                None, }, cfg::RegistryEntry { id : "..:plains".into(), data : None, },
            ],
        };
        fill_known_entries(&mut reg, &resources);
        assert!(reg.entries[0].data.is_some());
        assert!(reg.entries[1].data.is_none());
        assert!(reg.entries[2].data.is_none());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
pub mod sunset_client_brand {
    use horizon_protocol::packets::play::Clientbound as C;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard};
    pub struct Plan {
        pub brand: String,
        pub profile: &'static str,
        pub host: String,
        pub notes: Vec<&'static str>,
        pub joins: Vec<(&'static str, Vec<u8>)>,
    }
    static NEXT: Mutex<Option<Plan>> = Mutex::new(None);
    static CURRENT: Mutex<Option<Arc<Plan>>> = Mutex::new(None);
    static REPORT: AtomicBool = AtomicBool::new(false);
    static JOINS: AtomicBool = AtomicBool::new(false);
    static LOGIN: AtomicBool = AtomicBool::new(false);
    fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
        m.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn next(plan: Option<Plan>) {
        *lock(&NEXT) = plan;
        *lock(&CURRENT) = None;
        REPORT.store(false, Ordering::Relaxed);
        JOINS.store(false, Ordering::Relaxed);
        LOGIN.store(false, Ordering::Relaxed);
    }
    pub fn payload() -> Option<Vec<u8>> {
        let plan = lock(&NEXT).take()?;
        let mut w = horizon_protocol::buf::Writer::new();
        w.string(&plan.brand);
        JOINS.store(!plan.joins.is_empty(), Ordering::Release);
        *lock(&CURRENT) = Some(Arc::new(plan));
        REPORT.store(true, Ordering::Release);
        Some(w.data)
    }
    pub fn seen(events: &[super::Event]) {
        if JOINS.load(Ordering::Acquire)
            && events
                .iter()
                .any(|e| {
                    matches!(e, super::Event::Packet(p) if matches!(** p, C::Login(_)))
                })
        {
            LOGIN.store(true, Ordering::Release);
        }
    }
    pub fn joins() -> Option<Arc<Plan>> {
        if !LOGIN.swap(false, Ordering::Acquire) {
            return None;
        }
        lock(&CURRENT).clone()
    }
    pub fn sent() -> Option<Arc<Plan>> {
        if !REPORT.swap(false, Ordering::Acquire) {
            return None;
        }
        lock(&CURRENT).clone()
    }
}
pub mod sunset_packets {
    use horizon_protocol::packets::play::Clientbound as C;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex, Weak};
    use std::time::{Duration, Instant};
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    static EPOCH: AtomicU64 = AtomicU64::new(0);
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    static LAST_SESSION: Mutex<Option<Weak<CountersCell>>> = Mutex::new(None);
    type CountersCell = Mutex<Option<Arc<horizon_protocol::connection::Counters>>>;
    #[derive(Clone, Debug)]
    pub struct Link {
        cell: Weak<CountersCell>,
        pub serial: u64,
        pub epoch: u64,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Linked {
        Same,
        Reconfigured,
        Other,
    }
    impl Link {
        pub fn of(session: &super::Session) -> Self {
            Self {
                cell: Arc::downgrade(&session.counters),
                serial: SERIAL.load(Ordering::Acquire),
                epoch: EPOCH.load(Ordering::Acquire),
            }
        }
        pub fn to(&self, session: Option<&super::Session>) -> Linked {
            match session {
                Some(s) if std::ptr::eq(self.cell.as_ptr(), Arc::as_ptr(&s.counters)) => {
                    if self.epoch == EPOCH.load(Ordering::Acquire) {
                        Linked::Same
                    } else {
                        Linked::Reconfigured
                    }
                }
                _ => Linked::Other,
            }
        }
    }
    fn boundary(event: &super::Event) -> bool {
        matches!(
            event, super::Event::Joined { .. } | super::Event::Reconfiguring |
            super::Event::Reconfigured(_) | super::Event::Disconnected { .. }
        )
    }
    pub fn note(session: &super::Session, events: &[super::Event]) {
        {
            let mut last = LAST_SESSION.lock().unwrap_or_else(|e| e.into_inner());
            if last
                .as_ref()
                .is_none_or(|w| {
                    !std::ptr::eq(w.as_ptr(), Arc::as_ptr(&session.counters))
                })
            {
                *last = Some(Arc::downgrade(&session.counters));
                SERIAL.fetch_add(1, Ordering::AcqRel);
            }
        }
        let n = events.iter().filter(|e| boundary(e)).count() as u64;
        if n > 0 {
            EPOCH.fetch_add(n, Ordering::AcqRel);
        }
    }
    #[derive(Clone, Copy, Debug, Default)]
    pub struct QueueStats {
        pub held: u64,
        pub released: u64,
        pub flushed: [u64; 7],
        pub discarded_session: u64,
        pub discarded_life: u64,
        pub max_depth: usize,
        pub depth: usize,
        pub target_switches: u64,
    }
    pub const FLUSH_REASONS: [&str; 7] = [
        "connection boundary",
        "teleport",
        "own velocity",
        "target removed",
        "target switch",
        "not moving away",
        "off",
    ];
    fn flushed<T>(
        held: &mut VecDeque<(Instant, T)>,
        stats: &mut QueueStats,
        reason: usize,
        out: &mut Vec<T>,
    ) {
        if held.is_empty() {
            return;
        }
        stats.flushed[reason] += held.len() as u64;
        stats.released += held.len() as u64;
        out.extend(held.drain(..).map(|(_, e)| e));
    }
    static GUI_MOVE_ACTIVE: AtomicBool = AtomicBool::new(false);
    static GUI_MOVE_UPDATES: Mutex<Vec<C>> = Mutex::new(Vec::new());
    pub fn gui_move_active(active: bool) {
        GUI_MOVE_ACTIVE.store(active, Ordering::Release);
    }
    pub fn gui_move_take() -> Vec<C> {
        std::mem::take(&mut *GUI_MOVE_UPDATES.lock().unwrap_or_else(|e| e.into_inner()))
    }
    pub fn gui_move_pending() -> bool {
        !GUI_MOVE_UPDATES.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
    }
    fn gui_move_key(packet: &C) -> Option<(u8, i32)> {
        match packet {
            C::ContainerSetContent(p) if p.container == 0 => Some((0, 0)),
            C::ContainerSetSlot(p) if p.container == 0 && (0..46).contains(&p.slot) => {
                Some((1, i32::from(p.slot)))
            }
            C::SetPlayerInventory(p) if (0..41).contains(&p.slot) => Some((2, p.slot)),
            C::SetCursorItem(_) => Some((3, 0)),
            _ => None,
        }
    }
    fn gui_move_defer(holding: &mut bool, boundary: bool, inventory: bool) -> bool {
        if boundary {
            *holding = false;
        }
        *holding && inventory
    }
    #[cfg(test)]
    mod gui_move_boundary_tests {
        use super::gui_move_defer;
        #[test]
        fn later_inventory_authority_keeps_its_order_after_a_menu_boundary() {
            let mut holding = true;
            assert!(gui_move_defer(& mut holding, false, true));
            assert!(! gui_move_defer(& mut holding, true, false));
            assert!(! gui_move_defer(& mut holding, false, false));
            assert!(! gui_move_defer(& mut holding, false, true));
        }
    }
    pub fn gui_move_hold(events: Vec<super::Event>) -> Vec<super::Event> {
        if !GUI_MOVE_ACTIVE.load(Ordering::Acquire) {
            return events;
        }
        let mut holding = true;
        events
            .into_iter()
            .filter(|event| {
                let super::Event::Packet(packet) = event else {
                    return true;
                };
                let boundary = matches!(
                    &** packet, C::OpenScreen(_) | C::MountScreenOpen(_) |
                    C::ContainerClose(_) | C::Respawn(_) | C::Login(_) | C::Disconnect(_)
                );
                if boundary {
                    GUI_MOVE_ACTIVE.store(false, Ordering::Release);
                }
                let key = gui_move_key(packet);
                if !gui_move_defer(&mut holding, boundary, key.is_some()) {
                    return true;
                }
                let Some(key) = key else {
                    return true;
                };
                let mut updates = GUI_MOVE_UPDATES
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if key == (0, 0) {
                    updates.clear();
                } else {
                    updates.retain(|old| gui_move_key(old) != Some(key));
                }
                updates.push((**packet).clone());
                false
            })
            .collect()
    }
    #[derive(Debug)]
    pub enum ScanEvent {
        ConfigChannel(u32, String),
        Brand(u32, String),
        Channel(u32, String),
        Commands(u32),
        Generation(u32),
        Overflow,
    }
    static SCAN_ACTIVE: AtomicBool = AtomicBool::new(false);
    static SCAN: Mutex<VecDeque<ScanEvent>> = Mutex::new(VecDeque::new());
    static PLAY_GEN: AtomicU32 = AtomicU32::new(0);
    static BRAND_WAITING: AtomicBool = AtomicBool::new(false);
    static BRAND: Mutex<Option<String>> = Mutex::new(None);
    pub fn scan_enable(on: bool) {
        SCAN_ACTIVE.store(on, Ordering::Relaxed);
        if !on {
            SCAN.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
    }
    pub fn scan_clear() {
        PLAY_GEN.store(0, Ordering::Relaxed);
        BRAND_WAITING.store(false, Ordering::Relaxed);
        SCAN.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
    pub fn scan_generation() -> u32 {
        PLAY_GEN.load(Ordering::Relaxed)
    }
    pub fn scan_configuring(session: &super::Session) -> bool {
        session.configuring.load(Ordering::Relaxed)
    }
    fn scan_push(event: ScanEvent) {
        let mut queue = SCAN.lock().unwrap_or_else(|e| e.into_inner());
        scan_push_to(&mut queue, event);
    }
    fn scan_push_to(queue: &mut VecDeque<ScanEvent>, event: ScanEvent) {
        if queue.len() == 256 {
            queue.clear();
            queue.push_back(ScanEvent::Overflow);
        }
        queue.push_back(event);
    }
    fn channel_id(id: &str) -> bool {
        !id.is_empty() && id.len() <= 128 && id.is_ascii()
    }
    fn registered(data: &[u8], mut push: impl FnMut(String)) {
        for id in data.split(|&b| b == 0).take(64) {
            if let Ok(id) = std::str::from_utf8(id) && channel_id(id) {
                push(id.to_owned());
            }
        }
    }
    pub fn scan_config(
        payload: &horizon_protocol::packets::configuration::CustomPayload,
    ) {
        if !SCAN_ACTIVE.load(Ordering::Relaxed) {
            return;
        }
        let generation = PLAY_GEN.load(Ordering::Relaxed) + 1;
        match payload.channel.as_str() {
            "minecraft:brand" | "minecraft:unregister" => {}
            "minecraft:register" => {
                registered(
                    &payload.data,
                    |id| scan_push(ScanEvent::ConfigChannel(generation, id)),
                )
            }
            id if channel_id(id) => {
                scan_push(ScanEvent::ConfigChannel(generation, id.to_owned()))
            }
            _ => {}
        }
    }
    pub fn scan_take() -> VecDeque<ScanEvent> {
        std::mem::take(&mut *SCAN.lock().unwrap_or_else(|e| e.into_inner()))
    }
    fn brand_to(generation: u32) {
        if BRAND_WAITING.swap(false, Ordering::Relaxed)
            && let Some(brand) = BRAND.lock().unwrap_or_else(|e| e.into_inner()).take()
        {
            scan_push(ScanEvent::Brand(generation, brand));
        }
    }
    fn scan_seen(event: &super::Event, active: bool) {
        let generation = match event {
            super::Event::Joined { .. } | super::Event::Reconfigured(_) => {
                PLAY_GEN.fetch_add(1, Ordering::Relaxed) + 1
            }
            _ => PLAY_GEN.load(Ordering::Relaxed),
        };
        if !active {
            return;
        }
        match event {
            super::Event::Brand(brand) => {
                *BRAND.lock().unwrap_or_else(|e| e.into_inner()) = Some(
                    brand.chars().take(128).collect(),
                );
                BRAND_WAITING.store(true, Ordering::Relaxed);
            }
            super::Event::Joined { .. } | super::Event::Reconfigured(_) => {
                brand_to(generation);
                scan_push(ScanEvent::Generation(generation));
            }
            super::Event::Packet(packet) => {
                match &**packet {
                    _ if BRAND_WAITING.load(Ordering::Relaxed) => {
                        brand_to(generation);
                        scan_seen(event, active);
                    }
                    C::Commands(_) => scan_push(ScanEvent::Commands(generation)),
                    C::CustomPayload(
                        payload,
                    ) if payload.channel == "minecraft:register" => {
                        registered(
                            &payload.data,
                            |id| scan_push(ScanEvent::Channel(generation, id)),
                        )
                    }
                    C::CustomPayload(
                        payload,
                    ) if payload.channel != "minecraft:unregister"
                        && channel_id(&payload.channel) => {
                        scan_push(
                            ScanEvent::Channel(generation, payload.channel.clone()),
                        )
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    #[cfg(test)]
    mod scan_tests {
        use super::*;
        #[test]
        fn scan_queue_is_bounded_and_marks_lost_evidence() {
            let mut queue = VecDeque::new();
            for _ in 0..1024 {
                scan_push_to(&mut queue, ScanEvent::Commands(1));
            }
            assert!(queue.len() <= 256);
            assert!(queue.iter().any(| event | matches!(event, ScanEvent::Overflow)));
        }
        #[test]
        fn register_payload_is_split_and_bounded() {
            let mut out = Vec::new();
            registered(b"worldedit:cui\0bungeecord:main\0\0\xff\xfe", |id| out.push(id));
            assert_eq!(out, ["worldedit:cui", "bungeecord:main"]);
            let mut many = 0;
            registered(&b"a:b\0".repeat(500), |_| many += 1);
            assert_eq!(many, 64);
        }
    }
    static SEEN: Mutex<Vec<C>> = Mutex::new(Vec::new());
    pub static PLUGIN_PACKETS: AtomicBool = AtomicBool::new(false);
    static PLUGIN_IN: Mutex<VecDeque<C>> = Mutex::new(VecDeque::new());
    pub enum Connection {
        Joined,
        Left(String, Option<String>, bool),
    }
    static PLUGIN_CONNECTION: Mutex<Vec<Connection>> = Mutex::new(Vec::new());
    pub fn plugin_packets() -> Vec<C> {
        PLUGIN_IN.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect()
    }
    pub fn plugin_connection() -> Vec<Connection> {
        std::mem::take(&mut *PLUGIN_CONNECTION.lock().unwrap_or_else(|e| e.into_inner()))
    }
    fn plugin_seen(event: &super::Event) {
        match event {
            super::Event::Joined { .. } => {
                PLUGIN_CONNECTION
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(Connection::Joined)
            }
            super::Event::Disconnected { reason, key, from_server } => {
                PLUGIN_CONNECTION
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(
                        Connection::Left(
                            reason.chars().take(512).collect(),
                            key.clone(),
                            *from_server,
                        ),
                    )
            }
            super::Event::Packet(p) if PLUGIN_PACKETS.load(Ordering::Relaxed) => {
                let mut queue = PLUGIN_IN.lock().unwrap_or_else(|e| e.into_inner());
                if queue.len() == 4096 {
                    queue.pop_front();
                }
                queue.push_back((**p).clone());
            }
            _ => {}
        }
    }
    static CLOCK: Mutex<VecDeque<(Instant, i64)>> = Mutex::new(VecDeque::new());
    static ENDS: Mutex<VecDeque<(std::time::SystemTime, String, bool)>> = Mutex::new(
        VecDeque::new(),
    );
    fn debug_seen(event: &super::Event) {
        match event {
            super::Event::Joined { .. } => {
                CLOCK.lock().unwrap_or_else(|e| e.into_inner()).clear()
            }
            super::Event::Disconnected { reason, from_server, .. } => {
                let mut ends = ENDS.lock().unwrap_or_else(|e| e.into_inner());
                if ends.len() == 20 {
                    ends.pop_front();
                }
                ends.push_back((
                    std::time::SystemTime::now(),
                    reason.chars().take(512).collect(),
                    *from_server,
                ));
            }
            super::Event::Packet(p) => {
                if let C::SetTime(t) = &**p {
                    let mut clock = CLOCK.lock().unwrap_or_else(|e| e.into_inner());
                    if clock.len() == 30 {
                        clock.pop_front();
                    }
                    clock.push_back((Instant::now(), t.game_time));
                }
            }
            _ => {}
        }
    }
    pub fn tps() -> Option<(f64, f64)> {
        let clock = CLOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (first, last) = (clock.front()?, clock.back()?);
        let seconds = last.0.duration_since(first.0).as_secs_f64();
        (seconds >= 3.0).then(|| ((last.1 - first.1) as f64 / seconds, seconds))
    }
    pub fn ends() -> Vec<(std::time::SystemTime, String, bool)> {
        ENDS.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
    }
    pub fn seen(events: &[super::Event]) {
        let scan_active = SCAN_ACTIVE.load(Ordering::Relaxed);
        let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
        for event in events {
            scan_seen(event, scan_active);
            debug_seen(event);
            plugin_seen(event);
            if let super::Event::Packet(p) = event
                && matches!(
                    ** p, C::Login(_) | C::Respawn(_) | C::Disconnect(_) |
                    C::PlayerPosition(_) | C::BlockUpdate(_) | C::SectionBlocksUpdate(_)
                    | C::AddEntity(_) | C::RemoveEntities(_) | C::SetEntityData(_) |
                    C::PlayerInfoUpdate(_) | C::PlayerInfoRemove(_) | C::SetEquipment(_)
                    | C::DamageEvent(_) | C::EntityEvent(_) | C::SetEntityMotion(_) |
                    C::ContainerSetSlot(_) | C::ContainerSetContent(_) |
                    C::SetPlayerInventory(_) | C::SetCursorItem(_) | C::ContainerClose(_)
                    | C::OpenScreen(_)
                ) && seen.len() < 4096
            {
                seen.push((**p).clone());
            }
            if let super::Event::Packet(p) = event && SOUNDS_ON.load(Ordering::Relaxed)
                && matches!(
                    ** p, C::Sound(_) | C::SoundEntity(_) | C::BlockEvent(_) |
                    C::LevelEvent(_) | C::BlockDestruction(_) | C::TakeItemEntity(_) |
                    C::Explode(_)
                )
            {
                let mut sounds = SOUNDS.lock().unwrap_or_else(|e| e.into_inner());
                if sounds.len() < 512 {
                    sounds.push((**p).clone());
                }
            }
        }
    }
    static SOUNDS_ON: AtomicBool = AtomicBool::new(false);
    static SOUNDS: Mutex<Vec<C>> = Mutex::new(Vec::new());
    pub fn sounds_on(on: bool) {
        if SOUNDS_ON.swap(on, Ordering::Relaxed) && !on {
            SOUNDS.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
    }
    pub fn take_sounds() -> Vec<C> {
        std::mem::take(&mut *SOUNDS.lock().unwrap_or_else(|e| e.into_inner()))
    }
    struct Trap {
        kinds: Vec<i32>,
        at: [f64; 3],
        range: f64,
        sign: bool,
        told: Instant,
    }
    static TRAP: Mutex<Option<Trap>> = Mutex::new(None);
    pub fn trap(kinds: Vec<i32>, at: [f64; 3], range: f64, sign: bool) {
        *TRAP.lock().unwrap_or_else(|e| e.into_inner()) = Some(Trap {
            kinds,
            at,
            range,
            sign,
            told: Instant::now(),
        });
    }
    pub fn trap_off() {
        *TRAP.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    pub fn trap_filter(mut events: Vec<super::Event>) -> Vec<super::Event> {
        let guard = TRAP.lock().unwrap_or_else(|e| e.into_inner());
        let Some(trap) = guard
            .as_ref()
            .filter(|t| t.told.elapsed() < Duration::from_millis(250)) else {
            return events;
        };
        events
            .retain(|event| {
                let super::Event::Packet(p) = event else {
                    return true;
                };
                match &**p {
                    C::OpenSignEditor(_) => !trap.sign,
                    C::AddEntity(a) if trap.kinds.contains(&a.kind) => {
                        let d2: f64 = (0..3)
                            .map(|k| (a.position[k] - trap.at[k]).powi(2))
                            .sum();
                        d2 > trap.range * trap.range
                    }
                    _ => true,
                }
            });
        events
    }
    pub fn take() -> Vec<C> {
        std::mem::take(&mut *SEEN.lock().unwrap_or_else(|e| e.into_inner()))
    }
    struct Delay {
        me: i32,
        chance: f64,
        wait: [u64; 2],
        told: Instant,
        link: Option<Link>,
        held: VecDeque<(Instant, super::Event)>,
        roll: u64,
        stats: QueueStats,
    }
    static DELAY: Mutex<Option<Delay>> = Mutex::new(None);
    pub fn delay(me: i32, chance: f64, wait: [u64; 2]) {
        let mut d = DELAY.lock().unwrap_or_else(|e| e.into_inner());
        let d = d
            .get_or_insert_with(|| Delay {
                me,
                chance,
                wait,
                told: Instant::now(),
                link: None,
                held: VecDeque::new(),
                roll: 0x9e37_79b9_7f4a_7c15,
                stats: QueueStats::default(),
            });
        (d.me, d.chance, d.wait, d.told) = (me, chance, wait, Instant::now());
    }
    pub fn delay_off() {
        if let Some(d) = DELAY.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            d.told -= Duration::from_secs(1);
        }
    }
    pub fn delay_stats() -> Option<QueueStats> {
        DELAY
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|d| QueueStats {
                depth: d.held.len(),
                ..d.stats
            })
    }
    fn rebind(
        link: &mut Option<Link>,
        session: &super::Session,
        held: usize,
        stats: &mut QueueStats,
    ) -> bool {
        let stale = link.as_ref().is_some_and(|l| l.to(Some(session)) == Linked::Other);
        if stale {
            stats.discarded_session += held as u64;
        }
        if link.is_none() || stale {
            *link = Some(Link::of(session));
        }
        stale
    }
    pub fn hold(
        session: &super::Session,
        events: Vec<super::Event>,
    ) -> Vec<super::Event> {
        let mut guard = DELAY.lock().unwrap_or_else(|e| e.into_inner());
        let Some(d) = guard.as_mut() else {
            return events;
        };
        if rebind(&mut d.link, session, d.held.len(), &mut d.stats) {
            d.held.clear();
        }
        let now = Instant::now();
        let on = horizon_protocol::connection::sunset_lifecycle::RUNTIME.running()
            && now - d.told < Duration::from_millis(250);
        if d.held.is_empty() && !on {
            return events;
        }
        let mut out = Vec::with_capacity(events.len() + d.held.len());
        let due = d.held.iter().take_while(|(at, _)| !on || *at <= now).count();
        d.stats.released += due as u64;
        out.extend(d.held.drain(..due).map(|(_, e)| e));
        for event in events {
            let super::Event::Packet(p) = &event else {
                if boundary(&event) {
                    flushed(&mut d.held, &mut d.stats, 0, &mut out);
                }
                out.push(event);
                continue;
            };
            match &**p {
                C::Login(_) | C::Respawn(_) => {
                    d.stats.discarded_life += d.held.len() as u64;
                    d.held.clear();
                }
                C::PlayerPosition(_) => flushed(&mut d.held, &mut d.stats, 1, &mut out),
                C::SetEntityMotion(m) if m.id == d.me => {}
                C::Explode(x) if x.knockback.is_some() => {}
                _ => {
                    out.push(event);
                    continue;
                }
            }
            if on && matches!(&** p, C::SetEntityMotion(_) | C::Explode(_)) {
                let at = if let Some((last, _)) = d.held.back() {
                    Some(*last)
                } else {
                    d.roll ^= d.roll << 13;
                    d.roll ^= d.roll >> 7;
                    d.roll ^= d.roll << 17;
                    let [lo, hi] = d.wait;
                    let lucky = ((d.roll % 10_000) as f64) < d.chance * 100.0;
                    (lucky && hi > 0)
                        .then(|| {
                            now
                                + Duration::from_millis(lo + (d.roll >> 16) % (hi - lo + 1))
                        })
                };
                if let Some(at) = at {
                    d.held.push_back((at, event));
                    d.stats.held += 1;
                    d.stats.max_depth = d.stats.max_depth.max(d.held.len());
                    continue;
                }
            }
            flushed(&mut d.held, &mut d.stats, 6, &mut out);
            out.push(event);
        }
        out
    }
    struct Backtrack {
        me: i32,
        target: i32,
        eye: [f64; 3],
        latency: [u64; 2],
        told: Instant,
        link: Option<Link>,
        server: Option<[f64; 3]>,
        switched: bool,
        away: bool,
        last_due: Option<Instant>,
        held: VecDeque<(Instant, super::Event)>,
        roll: u64,
        stats: QueueStats,
    }
    static BACKTRACK: Mutex<Option<Backtrack>> = Mutex::new(None);
    pub fn backtrack(
        me: i32,
        target: i32,
        at: [f64; 3],
        eye: [f64; 3],
        latency: [u64; 2],
    ) {
        let mut b = BACKTRACK.lock().unwrap_or_else(|e| e.into_inner());
        let b = b
            .get_or_insert_with(|| Backtrack {
                me,
                target,
                eye,
                latency,
                told: Instant::now(),
                link: None,
                server: None,
                switched: false,
                away: false,
                last_due: None,
                held: VecDeque::new(),
                roll: 0x2545_f491_4f6c_dd1d,
                stats: QueueStats::default(),
            });
        if b.target != target {
            b.switched = !b.held.is_empty();
            b.server = None;
            b.away = false;
            b.stats.target_switches += 1;
        } else if b.server.is_none() && b.held.is_empty() && !b.switched {
            b.server = Some(at);
        }
        (b.me, b.target, b.eye, b.latency, b.told) = (
            me,
            target,
            eye,
            latency,
            Instant::now(),
        );
    }
    pub fn backtrack_off() {
        if let Some(b) = BACKTRACK.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            b.told -= Duration::from_secs(1);
        }
    }
    pub fn backtrack_shown() -> Option<[f64; 3]> {
        BACKTRACK
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .filter(|b| !b.held.is_empty())
            .and_then(|b| b.server)
    }
    pub fn backtrack_stats() -> Option<QueueStats> {
        BACKTRACK
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|b| QueueStats {
                depth: b.held.len(),
                ..b.stats
            })
    }
    fn far(eye: [f64; 3], at: [f64; 3]) -> f64 {
        let centre = [at[0], at[1] + 0.9, at[2]];
        (0..3).map(|i| (centre[i] - eye[i]).powi(2)).sum::<f64>().sqrt()
    }
    pub fn backtrack_hold(
        session: &super::Session,
        events: Vec<super::Event>,
    ) -> Vec<super::Event> {
        let mut guard = BACKTRACK.lock().unwrap_or_else(|e| e.into_inner());
        let Some(b) = guard.as_mut() else {
            return events;
        };
        if rebind(&mut b.link, session, b.held.len(), &mut b.stats) {
            b.held.clear();
            b.server = None;
            b.away = false;
            b.switched = false;
        }
        let now = Instant::now();
        let on = horizon_protocol::connection::sunset_lifecycle::RUNTIME.running()
            && now - b.told < Duration::from_millis(250);
        if b.held.is_empty() && !on {
            b.server = None;
            b.switched = false;
            return events;
        }
        let mut out = Vec::with_capacity(events.len() + b.held.len());
        if b.switched {
            flushed(&mut b.held, &mut b.stats, 4, &mut out);
            b.switched = false;
        }
        let due = b.held.iter().take_while(|(at, _)| on && *at <= now).count();
        if on && b.held.len() < 4096 {
            b.stats.released += due as u64;
            out.extend(b.held.drain(..due).map(|(_, e)| e));
        } else {
            flushed(&mut b.held, &mut b.stats, 6, &mut out);
        }
        for event in events {
            let super::Event::Packet(p) = &event else {
                flushed(&mut b.held, &mut b.stats, 0, &mut out);
                b.away = false;
                if boundary(&event) {
                    b.server = None;
                }
                out.push(event);
                continue;
            };
            let flush = match &**p {
                C::Login(_) | C::Respawn(_) | C::Disconnect(_) => Some(0),
                C::PlayerPosition(_) => Some(1),
                C::SetEntityMotion(m) if m.id == b.me => Some(2),
                C::RemoveEntities(r) if r.ids.contains(&b.target) => Some(3),
                _ => None,
            };
            if flush.is_some() || !on {
                flushed(&mut b.held, &mut b.stats, flush.unwrap_or(6), &mut out);
                b.away = false;
                if flush.is_some_and(|f| f != 2) {
                    b.server = None;
                }
                out.push(event);
                continue;
            }
            let moved = match &**p {
                C::MoveEntityPos(m) if m.0.id == b.target => {
                    b.server.zip(m.0.delta.as_ref()).map(|(at, d)| d.end(at))
                }
                C::MoveEntityPosRot(m) if m.0.id == b.target => {
                    b.server.zip(m.0.delta.as_ref()).map(|(at, d)| d.end(at))
                }
                C::TeleportEntity(t) if t.id == b.target && t.relatives == 0 => {
                    Some(t.position)
                }
                C::EntityPositionSync(s) if s.id == b.target => Some(s.path.end()),
                _ => None,
            };
            if let Some(at) = moved {
                b.away = b
                    .server
                    .is_some_and(|was| far(b.eye, at) > far(b.eye, was) + 0.001);
                b.server = Some(at);
            }
            if !b.away {
                flushed(&mut b.held, &mut b.stats, 5, &mut out);
                out.push(event);
                continue;
            }
            b.roll ^= b.roll << 13;
            b.roll ^= b.roll >> 7;
            b.roll ^= b.roll << 17;
            let [lo, hi] = b.latency;
            let mut at = now + Duration::from_millis(lo + b.roll % (hi - lo + 1));
            if let Some(last) = b.last_due.filter(|last| at <= *last) {
                at = last + Duration::from_micros(1);
            }
            b.last_due = Some(at);
            b.held.push_back((at, event));
            b.stats.held += 1;
            b.stats.max_depth = b.stats.max_depth.max(b.held.len());
        }
        out
    }
}
