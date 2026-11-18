package axs;

import java.io.*;
import java.lang.reflect.Field;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.zip.GZIPOutputStream;

import fr.soe.a3s.constant.EncryptionMode;
import fr.soe.a3s.constant.ProtocolType;
import fr.soe.a3s.domain.AbstractProtocole;
import fr.soe.a3s.domain.Ftp;
import fr.soe.a3s.domain.Http;
import fr.soe.a3s.domain.configration.FavoriteServer;
import fr.soe.a3s.domain.repository.*;

/**
 * Generates golden fixtures using the original Arma3Sync classes, written
 * exactly like the legacy app does (GZIP + ObjectOutputStream).
 * Usage: java axs.Gen outDir
 */
public final class Gen {

    public static void main(String[] args) throws Exception {
        File out = new File(args.length > 0 ? args[0] : "fixtures");
        out.mkdirs();
        emit(out, "sync_basic", syncBasic());
        emit(out, "sync_empty", new SyncTreeDirectory(SyncTreeNode.RACINE, null));
        emit(out, "sync_deep", syncDeep(40));
        emit(out, "serverinfo", serverInfo());
        emit(out, "serverinfo_empty", new ServerInfo());
        emit(out, "changelogs", changelogs(10));
        emit(out, "changelogs_empty", new Changelogs());
        emit(out, "events", events());
        emit(out, "events_empty", new Events());
        emit(out, "autoconfig_http", autoConfig(http()));
        emit(out, "autoconfig_ftp", autoConfig(ftp()));
        emit(out, "autoconfig_null", new AutoConfig());
        System.out.println("fixtures written to " + out.getAbsolutePath());
    }

    static void emit(File dir, String name, Object o) throws Exception {
        File f = new File(dir, name + ".a3s");
        try (ObjectOutputStream os = new ObjectOutputStream(new GZIPOutputStream(new FileOutputStream(f)))) {
            os.writeObject(o);
        }
        // Re-read through deserialization so the dump reflects what a reader sees.
        Object back = Dump.readA3s(f);
        try (Writer w = new OutputStreamWriter(new FileOutputStream(new File(dir, name + ".json")), StandardCharsets.UTF_8)) {
            w.write(Dump.toJson(back));
            w.write("\n");
        }
    }

    static void set(Object o, String field, Object value) throws Exception {
        Field f = Dump.findField(o.getClass(), field);
        f.setAccessible(true);
        f.set(o, value);
    }

    static SyncTreeLeaf leaf(SyncTreeDirectory parent, String name, String sha1, long size) throws Exception {
        SyncTreeLeaf l = new SyncTreeLeaf(new String(name), parent);
        set(l, "sha1", sha1 == null ? null : new String(sha1));
        set(l, "size", size);
        parent.addTreeNode(l);
        return l;
    }

    static SyncTreeDirectory dir(SyncTreeDirectory parent, String name, boolean addon) throws Exception {
        SyncTreeDirectory d = new SyncTreeDirectory(new String(name), parent);
        set(d, "markAsAddon", addon);
        parent.addTreeNode(d);
        return d;
    }

    static Object syncBasic() throws Exception {
        SyncTreeDirectory root = new SyncTreeDirectory(SyncTreeNode.RACINE, null);
        SyncTreeDirectory ace = dir(root, "@ace", true);
        SyncTreeDirectory aceAddons = dir(ace, "addons", false);
        SyncTreeLeaf pbo = leaf(aceAddons, "ace_main.pbo", "da39a3ee5e6b4b0d3255bfef95601890afd80709", 123456789L);
        set(pbo, "compressed", true);
        set(pbo, "compressedSize", 1000L);
        leaf(aceAddons, "ace_main.pbo.ace_3.15.bisign", "0123456789abcdef0123456789abcdef01234567", 570L);
        leaf(ace, "mod.cpp", "ffffffffffffffffffffffffffffffffffffffff", 0x1_0000_0001L);
        SyncTreeDirectory cba = dir(root, "@CBA_A3", true);
        dir(cba, "addons", false);
        dir(cba, "keys", false);
        SyncTreeDirectory uni = dir(root, "@Mód ñ 日本 🙂", true);
        dir(uni, "Addons", false);
        leaf(uni, "naïve file.txt", "1111111111111111111111111111111111111111", 3L);
        SyncTreeDirectory uc = dir(root, "userconfig", false);
        dir(uc, "ace", false);
        leaf(root, "readme.txt", "2222222222222222222222222222222222222222", 42L);
        leaf(root, "empty.txt", "0", 0L);
        SyncTreeLeaf flags = leaf(root, "flags.bin", "3333333333333333333333333333333333333333", 1L);
        set(flags, "updated", true);
        set(flags, "deleted", true);
        set(uc, "hidden", true);
        set(uc, "updated", true);
        return root;
    }

    static Object syncDeep(int depth) throws Exception {
        SyncTreeDirectory root = new SyncTreeDirectory(SyncTreeNode.RACINE, null);
        SyncTreeDirectory cur = root;
        for (int i = 0; i < depth; i++) {
            cur = dir(cur, "d" + i, i == 0);
            leaf(cur, "f" + i, String.format("%040x", i), i);
        }
        return root;
    }

    static Object serverInfo() throws Exception {
        ServerInfo s = new ServerInfo();
        set(s, "revision", 42);
        set(s, "buildDate", new Date(1_700_000_000_123L));
        set(s, "numberOfFiles", 1234L);
        set(s, "totalFilesSize", 98_765_432_101L);
        Set<String> hidden = new HashSet<String>(Arrays.asList("userconfig", "@ace/optionals", "a/b c/ñ"));
        set(s, "hiddenFolderPaths", hidden);
        set(s, "numberOfConnections", 4);
        set(s, "noPartialFileTransfer", true);
        set(s, "repositoryContentUpdated", true);
        set(s, "compressedPboFilesOnly", false);
        return s;
    }

    @SuppressWarnings("unchecked")
    static Object changelogs(int n) throws Exception {
        Changelogs cls = new Changelogs();
        List<Changelog> list = (List<Changelog>) getField(cls, "list");
        for (int i = 1; i <= n; i++) {
            Changelog c = new Changelog();
            set(c, "revision", i);
            set(c, "buildDate", new Date(1_600_000_000_000L + i * 86_400_000L));
            set(c, "contentUpdated", i % 2 == 0);
            ((List<String>) getField(c, "addons")).addAll(Arrays.asList(new String("@ace"), new String("@CBA_A3"), "@mod" + i));
            ((List<String>) getField(c, "newAddons")).add("@mod" + i);
            if (i > 1) ((List<String>) getField(c, "deletedAddons")).add("@mod" + (i - 1));
            if (i % 3 == 0) ((List<String>) getField(c, "updatedAddons")).add(new String("@ace"));
            list.add(c);
        }
        return cls;
    }

    @SuppressWarnings("unchecked")
    static Object events() throws Exception {
        Events evs = new Events();
        Event a = new Event("Sunday Op");
        set(a, "description", "Weekly operation \"Alpha\"");
        Map<String, Boolean> addons = (Map<String, Boolean>) getField(a, "addonNames");
        addons.put("@ace", false);
        addons.put("@CBA_A3", false);
        addons.put("@JSRS", true);
        Map<String, Boolean> uc = (Map<String, Boolean>) getField(a, "userconfigFolderNames");
        uc.put("ace", false);
        evs.list.add(a);
        Event b = new Event("Training");
        set(b, "description", null);
        set(b, "userconfigFolderNames", null);
        evs.list.add(b);
        return evs;
    }

    static AbstractProtocole http() {
        Http h = new Http("mods.example.com/r/main", "443", "player", "s3cr€t", ProtocolType.HTTPS);
        h.setConnectionTimeOut("300000");
        h.setReadTimeOut("0");
        return h;
    }

    static AbstractProtocole ftp() throws Exception {
        Ftp f = new Ftp("ftp.example.com/arma3", "21", "anonymous", "", ProtocolType.FTP);
        set(f, "encryptionMode", EncryptionMode.NO_ENCRYPTION);
        set(f, "connectionTimeOut", null);
        return f;
    }

    static Object autoConfig(AbstractProtocole p) throws Exception {
        AutoConfig a = new AutoConfig();
        set(a, "repositoryName", new String("Main Repo"));
        set(a, "protocole", p);
        List<FavoriteServer> servers = new ArrayList<FavoriteServer>();
        FavoriteServer s1 = new FavoriteServer();
        set(s1, "name", "Main Server");
        set(s1, "ipAddress", "203.0.113.10");
        set(s1, "port", 2302);
        set(s1, "password", "pw");
        set(s1, "modsetName", new String("Main Repo"));
        set(s1, "repositoryName", new String("Main Repo"));
        set(s1, "selected", true);
        servers.add(s1);
        FavoriteServer s2 = new FavoriteServer();
        set(s2, "name", "Training");
        set(s2, "ipAddress", "203.0.113.11");
        set(s2, "port", 2402);
        servers.add(s2);
        set(a, "favoriteServers", servers);
        return a;
    }

    static Object getField(Object o, String name) throws Exception {
        Field f = Dump.findField(o.getClass(), name);
        f.setAccessible(true);
        return f.get(o);
    }
}
