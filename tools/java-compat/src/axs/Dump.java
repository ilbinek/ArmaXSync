package axs;

import java.io.*;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.zip.GZIPInputStream;

/**
 * Canonical JSON dump of deserialized objects. The same rules are implemented
 * on the Rust side (axs-legacy canonical dump) so both sides can be compared:
 * <ul>
 * <li>objects: {"$class": fqcn, field: value...} over non-static, non-transient
 * fields of every Serializable class in the hierarchy;</li>
 * <li>a field named "parent" is rendered as the parent's "name" (breaks cycles);</li>
 * <li>List -> array, Set -> array sorted by element JSON, Map -> object;</li>
 * <li>Date -> {"$date": millis}, enum -> constant name, boxes -> primitives.</li>
 * </ul>
 * Usage: java axs.Dump file.a3s [...]   (gzip + ObjectInputStream, one line each)
 */
public final class Dump {

    public static void main(String[] args) throws Exception {
        PrintStream out = new PrintStream(new FileOutputStream(FileDescriptor.out), true, "UTF-8");
        int failures = 0;
        for (String a : args) {
            try {
                Object o = readA3s(new File(a));
                out.println(toJson(o));
            } catch (Throwable t) {
                failures++;
                System.err.println("FAILED " + a + ": " + t);
                t.printStackTrace();
                out.println("{\"$error\":" + str(t.toString()) + "}");
            }
        }
        System.exit(failures == 0 ? 0 : 1);
    }

    public static Object readA3s(File f) throws Exception {
        try (ObjectInputStream in = new ObjectInputStream(new GZIPInputStream(new BufferedInputStream(new FileInputStream(f))))) {
            Object o = in.readObject();
            if (in.read() != -1) throw new IOException("trailing data");
            return o;
        }
    }

    public static String toJson(Object o) throws Exception {
        StringBuilder sb = new StringBuilder();
        write(sb, o);
        return sb.toString();
    }

    static void write(StringBuilder sb, Object o) throws Exception {
        if (o == null) {
            sb.append("null");
        } else if (o instanceof String) {
            sb.append(str((String) o));
        } else if (o instanceof Boolean || o instanceof Integer || o instanceof Long || o instanceof Short
                || o instanceof Byte) {
            sb.append(o.toString());
        } else if (o instanceof Double || o instanceof Float) {
            double d = ((Number) o).doubleValue();
            if (d == Math.rint(d) && Math.abs(d) < 1e15) sb.append((long) d).append(".0");
            else sb.append(o.toString());
        } else if (o instanceof Character) {
            sb.append(str(o.toString()));
        } else if (o instanceof Enum) {
            sb.append(str(((Enum<?>) o).name()));
        } else if (o instanceof Date) {
            sb.append("{\"$date\":").append(((Date) o).getTime()).append('}');
        } else if (o instanceof List) {
            sb.append('[');
            boolean first = true;
            for (Object e : (List<?>) o) {
                if (!first) sb.append(',');
                first = false;
                write(sb, e);
            }
            sb.append(']');
        } else if (o instanceof Set) {
            List<String> parts = new ArrayList<String>();
            for (Object e : (Set<?>) o) {
                StringBuilder p = new StringBuilder();
                write(p, e);
                parts.add(p.toString());
            }
            Collections.sort(parts);
            sb.append('[');
            for (int i = 0; i < parts.size(); i++) {
                if (i > 0) sb.append(',');
                sb.append(parts.get(i));
            }
            sb.append(']');
        } else if (o instanceof Map) {
            TreeMap<String, Object> sorted = new TreeMap<String, Object>();
            for (Map.Entry<?, ?> e : ((Map<?, ?>) o).entrySet()) sorted.put(String.valueOf(e.getKey()), e.getValue());
            sb.append('{');
            boolean first = true;
            for (Map.Entry<String, Object> e : sorted.entrySet()) {
                if (!first) sb.append(',');
                first = false;
                sb.append(str(e.getKey())).append(':');
                write(sb, e.getValue());
            }
            sb.append('}');
        } else if (o.getClass().isArray()) {
            if (o instanceof byte[]) {
                sb.append("{\"$bytes\":").append(((byte[]) o).length).append('}');
            } else {
                sb.append('[');
                Object[] arr = (Object[]) o;
                for (int i = 0; i < arr.length; i++) {
                    if (i > 0) sb.append(',');
                    write(sb, arr[i]);
                }
                sb.append(']');
            }
        } else {
            TreeMap<String, Object> fields = new TreeMap<String, Object>();
            for (Class<?> c = o.getClass(); c != null && Serializable.class.isAssignableFrom(c); c = c.getSuperclass()) {
                for (Field f : c.getDeclaredFields()) {
                    int m = f.getModifiers();
                    if (Modifier.isStatic(m) || Modifier.isTransient(m)) continue;
                    f.setAccessible(true);
                    Object v = f.get(o);
                    if (f.getName().equals("parent") && v != null) {
                        Field nf = findField(v.getClass(), "name");
                        nf.setAccessible(true);
                        v = nf.get(v);
                    }
                    if (!fields.containsKey(f.getName())) fields.put(f.getName(), v);
                }
            }
            sb.append("{\"$class\":").append(str(o.getClass().getName()));
            for (Map.Entry<String, Object> e : fields.entrySet()) {
                sb.append(',').append(str(e.getKey())).append(':');
                write(sb, e.getValue());
            }
            sb.append('}');
        }
    }

    static Field findField(Class<?> c, String name) throws NoSuchFieldException {
        for (; c != null; c = c.getSuperclass()) {
            try {
                return c.getDeclaredField(name);
            } catch (NoSuchFieldException e) {
                // continue
            }
        }
        throw new NoSuchFieldException(name);
    }

    static String str(String s) {
        StringBuilder sb = new StringBuilder("\"");
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
            case '"': sb.append("\\\""); break;
            case '\\': sb.append("\\\\"); break;
            case '\n': sb.append("\\n"); break;
            case '\r': sb.append("\\r"); break;
            case '\t': sb.append("\\t"); break;
            case '\b': sb.append("\\b"); break;
            case '\f': sb.append("\\f"); break;
            default:
                if (c < 0x20) sb.append(String.format("\\u%04x", (int) c));
                else sb.append(c);
            }
        }
        return sb.append('"').toString();
    }

    static byte[] utf8(String s) {
        return s.getBytes(StandardCharsets.UTF_8);
    }
}
