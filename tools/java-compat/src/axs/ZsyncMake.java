package axs;

import java.io.File;

import fr.soe.a3s.jazsyncmake.MetaFileMaker;

/** Usage: java axs.ZsyncMake source target url sha1  (legacy jazsync maker) */
public final class ZsyncMake {
    public static void main(String[] a) throws Exception {
        new MetaFileMaker(new File(a[0]), new File(a[1]), a[2], a[3]);
    }
}
