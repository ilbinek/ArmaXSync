//! Reading and writing the `.a3s/*` repository metadata files.
//!
//! Readers are lenient (fields by name, defaults for anything missing,
//! serialVersionUIDs ignored). Writers emit exactly the legacy descriptors.

use axs_javaser::consts::*;
use axs_javaser::{ClassSpec, EnumSpec, FieldSpec, Graph, Handle, Value, Writer};

use crate::model::*;
use crate::{Error, Result};

// ---------------------------------------------------------------- specs ----

pub(crate) const T_STRING: &str = "Ljava/lang/String;";
pub(crate) const T_LIST: &str = "Ljava/util/List;";
pub(crate) const T_MAP: &str = "Ljava/util/Map;";
pub(crate) const T_SET: &str = "Ljava/util/Set;";
pub(crate) const T_DATE: &str = "Ljava/util/Date;";

pub const CLS_SYNC_DIR: &str = "fr.soe.a3s.domain.repository.SyncTreeDirectory";
pub const CLS_SYNC_LEAF: &str = "fr.soe.a3s.domain.repository.SyncTreeLeaf";
pub const CLS_SERVER_INFO: &str = "fr.soe.a3s.domain.repository.ServerInfo";
pub const CLS_CHANGELOGS: &str = "fr.soe.a3s.domain.repository.Changelogs";
pub const CLS_CHANGELOG: &str = "fr.soe.a3s.domain.repository.Changelog";
pub const CLS_EVENTS: &str = "fr.soe.a3s.domain.repository.Events";
pub const CLS_EVENT: &str = "fr.soe.a3s.domain.repository.Event";
pub const CLS_AUTOCONFIG: &str = "fr.soe.a3s.domain.repository.AutoConfig";
pub const CLS_FAVORITE_SERVER: &str = "fr.soe.a3s.domain.configration.FavoriteServer";
pub const CLS_PROTOCOLE: &str = "fr.soe.a3s.domain.AbstractProtocole";
pub const CLS_HTTP: &str = "fr.soe.a3s.domain.Http";
pub const CLS_FTP: &str = "fr.soe.a3s.domain.Ftp";
pub const CLS_SOCKS: &str = "fr.soe.a3s.domain.Socks";

const SER: u8 = SC_SERIALIZABLE;

pub static SYNC_DIR: ClassSpec = ClassSpec {
    name: CLS_SYNC_DIR,
    uid: -2855304993780573704,
    flags: SER,
    fields: &[
        FieldSpec::prim(b'Z', "deleted"),
        FieldSpec::prim(b'Z', "hidden"),
        FieldSpec::prim(b'Z', "markAsAddon"),
        FieldSpec::prim(b'Z', "updated"),
        FieldSpec::obj("list", T_LIST),
        FieldSpec::obj("name", T_STRING),
        FieldSpec::obj("parent", "Lfr/soe/a3s/domain/repository/SyncTreeDirectory;"),
    ],
    super_spec: None,
};

pub static SYNC_LEAF: ClassSpec = ClassSpec {
    name: CLS_SYNC_LEAF,
    uid: 8849248143660225239,
    flags: SER,
    fields: &[
        FieldSpec::prim(b'Z', "compressed"),
        FieldSpec::prim(b'J', "compressedSize"),
        FieldSpec::prim(b'Z', "deleted"),
        FieldSpec::prim(b'J', "size"),
        FieldSpec::prim(b'Z', "updated"),
        FieldSpec::obj("name", T_STRING),
        FieldSpec::obj("parent", "Lfr/soe/a3s/domain/repository/SyncTreeDirectory;"),
        FieldSpec::obj("sha1", T_STRING),
    ],
    super_spec: None,
};

pub static SERVER_INFO: ClassSpec = ClassSpec {
    name: CLS_SERVER_INFO,
    uid: 7697232677958952953,
    flags: SER,
    fields: &[
        FieldSpec::prim(b'Z', "compressedPboFilesOnly"),
        FieldSpec::prim(b'Z', "noPartialFileTransfer"),
        FieldSpec::prim(b'I', "numberOfConnections"),
        FieldSpec::prim(b'J', "numberOfFiles"),
        FieldSpec::prim(b'Z', "repositoryContentUpdated"),
        FieldSpec::prim(b'I', "revision"),
        FieldSpec::prim(b'J', "totalFilesSize"),
        FieldSpec::obj("buildDate", T_DATE),
        FieldSpec::obj("hiddenFolderPaths", T_SET),
    ],
    super_spec: None,
};

pub static CHANGELOGS: ClassSpec = ClassSpec {
    name: CLS_CHANGELOGS,
    uid: 7332983755625818660,
    flags: SER,
    fields: &[FieldSpec::obj("list", T_LIST)],
    super_spec: None,
};

pub static CHANGELOG: ClassSpec = ClassSpec {
    name: CLS_CHANGELOG,
    uid: 749128678979864247,
    flags: SER,
    fields: &[
        FieldSpec::prim(b'Z', "contentUpdated"),
        FieldSpec::prim(b'I', "revision"),
        FieldSpec::obj("addons", T_LIST),
        FieldSpec::obj("buildDate", T_DATE),
        FieldSpec::obj("deletedAddons", T_LIST),
        FieldSpec::obj("newAddons", T_LIST),
        FieldSpec::obj("updatedAddons", T_LIST),
    ],
    super_spec: None,
};

pub static EVENTS: ClassSpec = ClassSpec {
    name: CLS_EVENTS,
    uid: -5141643688299352462,
    flags: SER,
    fields: &[FieldSpec::obj("list", T_LIST)],
    super_spec: None,
};

pub static EVENT: ClassSpec = ClassSpec {
    name: CLS_EVENT,
    uid: 7456226002765813117,
    flags: SER,
    fields: &[
        FieldSpec::obj("addonNames", T_MAP),
        FieldSpec::obj("description", T_STRING),
        FieldSpec::obj("name", T_STRING),
        FieldSpec::obj("userconfigFolderNames", T_MAP),
    ],
    super_spec: None,
};

pub static AUTOCONFIG: ClassSpec = ClassSpec {
    name: CLS_AUTOCONFIG,
    uid: 2706280725619197755,
    flags: SER,
    fields: &[
        FieldSpec::obj("favoriteServers", T_LIST),
        FieldSpec::obj("protocole", "Lfr/soe/a3s/domain/AbstractProtocole;"),
        FieldSpec::obj("repositoryName", T_STRING),
    ],
    super_spec: None,
};

pub static FAVORITE_SERVER: ClassSpec = ClassSpec {
    name: CLS_FAVORITE_SERVER,
    uid: 4613236292596702132,
    flags: SER,
    fields: &[
        FieldSpec::prim(b'I', "port"),
        FieldSpec::prim(b'Z', "selected"),
        FieldSpec::obj("ipAddress", T_STRING),
        FieldSpec::obj("modsetName", T_STRING),
        FieldSpec::obj("name", T_STRING),
        FieldSpec::obj("password", T_STRING),
        FieldSpec::obj("repositoryName", T_STRING),
    ],
    super_spec: None,
};

pub static PROTOCOLE: ClassSpec = ClassSpec {
    name: CLS_PROTOCOLE,
    uid: -886357032287815203,
    flags: SER,
    fields: &[
        FieldSpec::obj("connectionTimeOut", T_STRING),
        FieldSpec::obj("encryptionMode", "Lfr/soe/a3s/constant/EncryptionMode;"),
        FieldSpec::obj("login", T_STRING),
        FieldSpec::obj("password", T_STRING),
        FieldSpec::obj("port", T_STRING),
        FieldSpec::obj("protocolType", "Lfr/soe/a3s/constant/ProtocolType;"),
        FieldSpec::obj("readTimeOut", T_STRING),
        FieldSpec::obj("url", T_STRING),
    ],
    super_spec: None,
};

pub static HTTP: ClassSpec = ClassSpec {
    name: CLS_HTTP,
    uid: -20,
    flags: SER,
    fields: &[],
    super_spec: Some(&PROTOCOLE),
};
pub static FTP: ClassSpec = ClassSpec {
    name: CLS_FTP,
    uid: -1917448655850030044,
    flags: SER,
    fields: &[],
    super_spec: Some(&PROTOCOLE),
};
pub static SOCKS: ClassSpec = ClassSpec {
    name: CLS_SOCKS,
    uid: -40,
    flags: SER,
    fields: &[],
    super_spec: Some(&PROTOCOLE),
};

pub static PROTOCOL_TYPE: EnumSpec = EnumSpec {
    name: "fr.soe.a3s.constant.ProtocolType",
};
pub static ENCRYPTION_MODE: EnumSpec = EnumSpec {
    name: "fr.soe.a3s.constant.EncryptionMode",
};

// --------------------------------------------------------------- reading ----

/// Helper for lenient typed reads.
pub(crate) struct R<'g> {
    pub g: &'g Graph,
}

impl<'g> R<'g> {
    pub fn expect_class(&self, h: Handle, class: &str) -> Result<()> {
        let n = self.g.class_name(h)?;
        if n != class {
            return Err(Error::UnexpectedClass {
                expected: class.to_owned(),
                found: n.to_owned(),
            });
        }
        Ok(())
    }
    fn field(&self, h: Handle, name: &str) -> Result<Option<&'g Value>> {
        Ok(self.g.field(h, name)?)
    }
    pub fn bool(&self, h: Handle, name: &str) -> Result<bool> {
        Ok(self
            .field(h, name)?
            .and_then(Value::as_bool)
            .unwrap_or(false))
    }
    pub fn i32(&self, h: Handle, name: &str, default: i32) -> Result<i32> {
        Ok(self
            .field(h, name)?
            .and_then(Value::as_i32)
            .unwrap_or(default))
    }
    pub fn i64(&self, h: Handle, name: &str) -> Result<i64> {
        Ok(self.field(h, name)?.and_then(Value::as_i64).unwrap_or(0))
    }
    pub fn f64(&self, h: Handle, name: &str) -> Result<f64> {
        Ok(self.field(h, name)?.and_then(Value::as_f64).unwrap_or(0.0))
    }
    pub fn obj(&self, h: Handle, name: &str) -> Result<Option<Handle>> {
        Ok(self.field(h, name)?.and_then(Value::as_ref))
    }
    pub fn string(&self, h: Handle, name: &str) -> Result<Option<String>> {
        match self.obj(h, name)? {
            Some(s) => Ok(Some(self.g.string(s)?.to_owned())),
            None => Ok(None),
        }
    }
    pub fn date(&self, h: Handle, name: &str) -> Result<Option<i64>> {
        match self.obj(h, name)? {
            Some(d) => Ok(self.g.date_millis(d)?),
            None => Ok(None),
        }
    }
    pub fn items(&self, coll: Option<Handle>) -> Result<Vec<Option<Handle>>> {
        match coll {
            None => Ok(vec![]),
            Some(c) => self.g.collection(c)?.ok_or_else(|| Error::UnexpectedClass {
                expected: "collection".into(),
                found: self.g.class_name(c).unwrap_or("?").to_owned(),
            }),
        }
    }
    pub fn strings(&self, coll: Option<Handle>) -> Result<Vec<String>> {
        self.items(coll)?
            .into_iter()
            .flatten()
            .map(|h| Ok(self.g.string(h)?.to_owned()))
            .collect()
    }
    pub fn string_list(&self, h: Handle, name: &str) -> Result<Vec<String>> {
        let c = self.obj(h, name)?;
        self.strings(c)
    }
    pub fn bool_map(&self, m: Option<Handle>) -> Result<Option<Vec<(String, bool)>>> {
        let Some(m) = m else { return Ok(None) };
        let pairs = self.g.map(m)?.ok_or_else(|| Error::UnexpectedClass {
            expected: "map".into(),
            found: self.g.class_name(m).unwrap_or("?").to_owned(),
        })?;
        let mut out = Vec::with_capacity(pairs.len());
        for (k, v) in pairs {
            let Some(k) = k else { continue };
            let key = self.g.string(k)?.to_owned();
            let val = match v {
                Some(v) => self.g.unbox(v)?.and_then(|v| v.as_bool()).unwrap_or(false),
                None => false,
            };
            out.push((key, val));
        }
        Ok(Some(out))
    }
    pub fn enum_name(&self, h: Option<Handle>) -> Result<Option<String>> {
        match h {
            Some(h) => Ok(Some(self.g.enum_constant(h)?.to_owned())),
            None => Ok(None),
        }
    }
}

pub(crate) fn root(g: &Graph) -> Result<Handle> {
    g.root().ok_or(Error::NullRoot)
}

pub fn read_sync_graph(g: &Graph) -> Result<SyncDir> {
    let r = R { g };
    let h = root(g)?;
    r.expect_class(h, CLS_SYNC_DIR)?;
    read_dir(&r, h, 0)
}

fn read_dir(r: &R, h: Handle, depth: usize) -> Result<SyncDir> {
    if depth > 1000 {
        return Err(Error::Invalid("sync tree too deep".into()));
    }
    let mut d = SyncDir {
        name: r.string(h, "name")?.unwrap_or_default(),
        mark_as_addon: r.bool(h, "markAsAddon")?,
        updated: r.bool(h, "updated")?,
        deleted: r.bool(h, "deleted")?,
        hidden: r.bool(h, "hidden")?,
        children: vec![],
    };
    for child in r.items(r.obj(h, "list")?)?.into_iter().flatten() {
        match r.g.class_name(child)? {
            CLS_SYNC_DIR => d
                .children
                .push(SyncNode::Dir(read_dir(r, child, depth + 1)?)),
            CLS_SYNC_LEAF => d.children.push(SyncNode::Leaf(SyncLeaf {
                name: r.string(child, "name")?.unwrap_or_default(),
                sha1: r.string(child, "sha1")?,
                size: r.i64(child, "size")?,
                compressed_size: r.i64(child, "compressedSize")?,
                compressed: r.bool(child, "compressed")?,
                updated: r.bool(child, "updated")?,
                deleted: r.bool(child, "deleted")?,
            })),
            other => {
                return Err(Error::UnexpectedClass {
                    expected: "sync tree node".into(),
                    found: other.to_owned(),
                })
            }
        }
    }
    Ok(d)
}

pub fn read_server_info_graph(g: &Graph) -> Result<ServerInfo> {
    let r = R { g };
    let h = root(g)?;
    r.expect_class(h, CLS_SERVER_INFO)?;
    let mut hidden = r.strings(r.obj(h, "hiddenFolderPaths")?)?;
    hidden.sort();
    Ok(ServerInfo {
        revision: r.i32(h, "revision", 0)?,
        build_date: r.date(h, "buildDate")?,
        number_of_files: r.i64(h, "numberOfFiles")?,
        total_files_size: r.i64(h, "totalFilesSize")?,
        hidden_folder_paths: hidden,
        number_of_connections: r.i32(h, "numberOfConnections", 1)?,
        no_partial_file_transfer: r.bool(h, "noPartialFileTransfer")?,
        repository_content_updated: r.bool(h, "repositoryContentUpdated")?,
        compressed_pbo_files_only: r.bool(h, "compressedPboFilesOnly")?,
    })
}

pub fn read_changelogs_graph(g: &Graph) -> Result<Changelogs> {
    let r = R { g };
    let h = root(g)?;
    r.expect_class(h, CLS_CHANGELOGS)?;
    let mut list = vec![];
    for c in r.items(r.obj(h, "list")?)?.into_iter().flatten() {
        r.expect_class(c, CLS_CHANGELOG)?;
        list.push(Changelog {
            revision: r.i32(c, "revision", 0)?,
            build_date: r.date(c, "buildDate")?,
            content_updated: r.bool(c, "contentUpdated")?,
            new_addons: r.string_list(c, "newAddons")?,
            updated_addons: r.string_list(c, "updatedAddons")?,
            deleted_addons: r.string_list(c, "deletedAddons")?,
            addons: r.string_list(c, "addons")?,
        });
    }
    Ok(Changelogs { list })
}

pub fn read_events_graph(g: &Graph) -> Result<Events> {
    let r = R { g };
    let h = root(g)?;
    r.expect_class(h, CLS_EVENTS)?;
    let mut list = vec![];
    for e in r.items(r.obj(h, "list")?)?.into_iter().flatten() {
        r.expect_class(e, CLS_EVENT)?;
        list.push(Event {
            name: r.string(e, "name")?,
            description: r.string(e, "description")?,
            addon_names: r.bool_map(r.obj(e, "addonNames")?)?.unwrap_or_default(),
            userconfig_folder_names: r.bool_map(r.obj(e, "userconfigFolderNames")?)?,
        });
    }
    Ok(Events { list })
}

pub fn read_autoconfig_graph(g: &Graph) -> Result<AutoConfig> {
    let r = R { g };
    let h = root(g)?;
    r.expect_class(h, CLS_AUTOCONFIG)?;
    let protocole = match r.obj(h, "protocole")? {
        Some(p) => Some(read_protocol(&r, p)?),
        None => None,
    };
    let mut favorite_servers = vec![];
    for s in r.items(r.obj(h, "favoriteServers")?)?.into_iter().flatten() {
        favorite_servers.push(read_favorite_server(&r, s)?);
    }
    Ok(AutoConfig {
        repository_name: r.string(h, "repositoryName")?,
        protocole,
        favorite_servers,
    })
}

pub(crate) fn read_favorite_server(r: &R, s: Handle) -> Result<FavoriteServer> {
    Ok(FavoriteServer {
        name: r.string(s, "name")?,
        ip_address: r.string(s, "ipAddress")?,
        port: r.i32(s, "port", 0)?,
        password: r.string(s, "password")?,
        selected: r.bool(s, "selected")?,
        modset_name: r.string(s, "modsetName")?,
        repository_name: r.string(s, "repositoryName")?,
    })
}

pub(crate) fn read_protocol(r: &R, p: Handle) -> Result<Protocol> {
    let class = match r.g.class_name(p)? {
        CLS_HTTP => ProtocolClass::Http,
        CLS_FTP => ProtocolClass::Ftp,
        CLS_SOCKS => ProtocolClass::Socks,
        other => {
            return Err(Error::UnexpectedClass {
                expected: "protocol".into(),
                found: other.to_owned(),
            })
        }
    };
    let pt = r.enum_name(r.obj(p, "protocolType")?)?;
    let em = r.enum_name(r.obj(p, "encryptionMode")?)?;
    Ok(Protocol {
        class,
        login: r.string(p, "login")?,
        password: r.string(p, "password")?,
        url: r.string(p, "url")?,
        port: r.string(p, "port")?,
        protocol_type: pt.as_deref().and_then(ProtocolType::from_java_name),
        encryption_mode: em.as_deref().and_then(EncryptionMode::from_java_name),
        connection_timeout: r.string(p, "connectionTimeOut")?,
        read_timeout: r.string(p, "readTimeOut")?,
    })
}

// --------------------------------------------------------------- writing ----

pub fn write_sync(w: &mut Writer, root: &SyncDir) {
    write_dir(w, root, None);
}

fn write_dir(w: &mut Writer, d: &SyncDir, parent: Option<Handle>) -> Handle {
    let h = w.begin_object(&SYNC_DIR);
    w.f_bool(d.deleted);
    w.f_bool(d.hidden);
    w.f_bool(d.mark_as_addon);
    w.f_bool(d.updated);
    w.begin_array_list(d.children.len());
    for c in &d.children {
        match c {
            SyncNode::Dir(sub) => {
                write_dir(w, sub, Some(h));
            }
            SyncNode::Leaf(l) => {
                w.begin_object(&SYNC_LEAF);
                w.f_bool(l.compressed);
                w.f_i64(l.compressed_size);
                w.f_bool(l.deleted);
                w.f_i64(l.size);
                w.f_bool(l.updated);
                w.string(&l.name);
                w.reference(h);
                w.opt_string(l.sha1.as_deref());
            }
        }
    }
    w.end_block();
    w.string(&d.name);
    match parent {
        Some(p) => w.reference(p),
        None => w.null(),
    }
    h
}

fn write_opt_date(w: &mut Writer, d: Option<i64>) {
    match d {
        Some(ms) => {
            w.date(ms);
        }
        None => w.null(),
    }
}

pub fn write_server_info(w: &mut Writer, s: &ServerInfo) {
    w.begin_object(&SERVER_INFO);
    w.f_bool(s.compressed_pbo_files_only);
    w.f_bool(s.no_partial_file_transfer);
    w.f_i32(s.number_of_connections);
    w.f_i64(s.number_of_files);
    w.f_bool(s.repository_content_updated);
    w.f_i32(s.revision);
    w.f_i64(s.total_files_size);
    write_opt_date(w, s.build_date);
    write_string_hash_set(w, &s.hidden_folder_paths);
}

pub fn write_changelogs(w: &mut Writer, c: &Changelogs) {
    w.begin_object(&CHANGELOGS);
    w.begin_array_list(c.list.len());
    for cl in &c.list {
        w.begin_object(&CHANGELOG);
        w.f_bool(cl.content_updated);
        w.f_i32(cl.revision);
        w.string_list(&cl.addons);
        write_opt_date(w, cl.build_date);
        w.string_list(&cl.deleted_addons);
        w.string_list(&cl.new_addons);
        w.string_list(&cl.updated_addons);
    }
    w.end_block();
}

pub fn write_events(w: &mut Writer, e: &Events) {
    w.begin_object(&EVENTS);
    w.begin_array_list(e.list.len());
    for ev in &e.list {
        w.begin_object(&EVENT);
        write_bool_map(w, Some(&ev.addon_names));
        w.opt_string(ev.description.as_deref());
        w.opt_string(ev.name.as_deref());
        write_bool_map(w, ev.userconfig_folder_names.as_deref());
    }
    w.end_block();
}

pub fn write_autoconfig(w: &mut Writer, a: &AutoConfig) {
    w.begin_object(&AUTOCONFIG);
    w.begin_array_list(a.favorite_servers.len());
    for s in &a.favorite_servers {
        write_favorite_server(w, s);
    }
    w.end_block();
    match &a.protocole {
        Some(p) => write_protocol(w, p),
        None => w.null(),
    }
    w.opt_string(a.repository_name.as_deref());
}

pub(crate) fn write_favorite_server(w: &mut Writer, s: &FavoriteServer) {
    w.begin_object(&FAVORITE_SERVER);
    w.f_i32(s.port);
    w.f_bool(s.selected);
    w.opt_string(s.ip_address.as_deref());
    w.opt_string(s.modset_name.as_deref());
    w.opt_string(s.name.as_deref());
    w.opt_string(s.password.as_deref());
    w.opt_string(s.repository_name.as_deref());
}

pub(crate) fn write_protocol(w: &mut Writer, p: &Protocol) {
    let spec = match p.class {
        ProtocolClass::Http => &HTTP,
        ProtocolClass::Ftp => &FTP,
        ProtocolClass::Socks => &SOCKS,
    };
    w.begin_object(spec);
    w.opt_string(p.connection_timeout.as_deref());
    match p.encryption_mode {
        Some(e) => {
            w.enum_const(&ENCRYPTION_MODE, e.java_name());
        }
        None => w.null(),
    }
    w.opt_string(p.login.as_deref());
    w.opt_string(p.password.as_deref());
    w.opt_string(p.port.as_deref());
    match p.protocol_type {
        Some(t) => {
            w.enum_const(&PROTOCOL_TYPE, t.java_name());
        }
        None => w.null(),
    }
    w.opt_string(p.read_timeout.as_deref());
    w.opt_string(p.url.as_deref());
    // Subclasses (Http/Ftp/Socks) declare no serializable fields.
}

/// Java's `String.hashCode`.
pub fn java_string_hash(s: &str) -> i32 {
    s.encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
}

/// Order keys the way a Java 8 `HashMap` with the given table size iterates.
pub(crate) fn java_hash_order<T>(items: &[T], key: impl Fn(&T) -> &str) -> Vec<&T> {
    let mut cap: usize = 16;
    while (cap as f64) * 0.75 < items.len() as f64 {
        cap *= 2;
    }
    let mut v: Vec<(usize, usize, &T)> = items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let h = java_string_hash(key(it)) as u32;
            let spread = h ^ (h >> 16);
            ((spread as usize) & (cap - 1), i, it)
        })
        .collect();
    v.sort_by_key(|(b, i, _)| (*b, *i));
    v.into_iter().map(|(_, _, it)| it).collect()
}

pub(crate) fn write_string_hash_set(w: &mut Writer, items: &[String]) {
    w.begin_hash_set(items.len());
    for s in java_hash_order(items, |s| s.as_str()) {
        w.string(s);
    }
    w.end_block();
}

pub(crate) fn write_bool_map(w: &mut Writer, m: Option<&[(String, bool)]>) {
    let Some(m) = m else {
        w.null();
        return;
    };
    w.begin_hash_map(m.len());
    for (k, v) in java_hash_order(m, |(k, _)| k.as_str()) {
        w.string(k);
        w.boxed_bool(*v);
    }
    w.end_block();
}
