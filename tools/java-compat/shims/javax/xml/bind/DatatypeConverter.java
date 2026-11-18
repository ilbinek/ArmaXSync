package javax.xml.bind;

/** Minimal shim for running the legacy client on Java 11+ (test harness only). */
public final class DatatypeConverter {
    private DatatypeConverter() {}
    public static String printBase64Binary(byte[] b) {
        return java.util.Base64.getEncoder().encodeToString(b);
    }
    public static byte[] parseBase64Binary(String s) {
        return java.util.Base64.getDecoder().decode(s);
    }
}
