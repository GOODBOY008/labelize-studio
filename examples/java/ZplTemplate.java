import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;
import java.util.Iterator;
import java.util.LinkedHashSet;
import java.util.Map;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/** Fills a .tpl.zpl exported by Labelize Studio. Requires only JDK 17. */
public final class ZplTemplate {
    private static final String MARKER = "^FXLabelize field ";
    private static final String DATA = "^FH^FD";
    private static final char[] HEX = "0123456789ABCDEF".toCharArray();
    private static final Pattern PART = Pattern.compile("(?:_[0-9A-F]{2})+|\\{\\{([a-zA-Z_][a-zA-Z_0-9]*)}}");
    private static final Pattern PDF417 = Pattern.compile("\\^B7[NRIB],[0-9]+,([0-8]),([0-9]*),,N");

    private ZplTemplate() {}

    public static byte[] fill(String template, Map<String, String> values) {
        StringBuilder job = new StringBuilder(template.length());
        Iterator<String> lines = template.lines().iterator();
        int fields = 0;
        while (lines.hasNext()) {
            String line = lines.next();
            if (line.startsWith(MARKER)) {
                require(line.endsWith("^FS") && lines.hasNext(), "Incomplete field marker");
                String kind = line.substring(MARKER.length(), line.length() - 3);
                line = fillField(lines.next(), kind, values);
                fields++;
            }
            job.append(line).append('\n');
        }
        require(fields > 0, "Choose a .tpl.zpl exported from the design workspace");
        return job.toString().getBytes(StandardCharsets.UTF_8);
    }

    private static String fillField(String line, String kind, Map<String, String> values) {
        int marker = line.indexOf(DATA);
        require(marker >= 0 && line.endsWith("^FS"), "Invalid " + kind + " field");
        int start = marker + DATA.length();
        String commands = line.substring(0, start);
        String expression = line.substring(start, line.length() - 3);
        String prefix = "";
        if (kind.equals("qr")) {
            require(expression.length() >= 3 && "LMQH".indexOf(expression.charAt(0)) >= 0
                    && expression.startsWith("A,", 1), "Invalid QR parameters");
            prefix = expression.substring(0, 3);
            expression = expression.substring(3);
        }
        Set<String> names = new LinkedHashSet<>();
        String value = expand(expression, values, names);
        try {
            require(value.length() <= 3072, "exceeds the ZPL field-data limit");
            require(StandardCharsets.UTF_8.newEncoder().canEncode(value), "invalid Unicode text");
            boolean caption = kind.endsWith("-caption");
            String type = caption ? kind.substring(0, kind.length() - "-caption".length()) : kind;
            if ((type.equals("text") || type.equals("block")) && !commands.contains("^A@")) {
                require(value.codePoints().allMatch(c -> c <= 255),
                        "set the printer font path in the design for Unicode text");
            }
            if (!type.equals("text") && !type.equals("block")) {
                require(!value.isEmpty(), "empty barcode");
            }
            switch (type) {
                case "text" -> require(!value.contains("\n") && !value.contains("\r"),
                        "enable text wrapping for multiline values");
                case "block" -> value = blockText(value);
                case "code39" -> require(value.matches("[A-Z0-9 .$%/+\\-]{1,512}"), "invalid Code 39 alphabet or length");
                case "itf" -> require(value.length() <= 512 && value.matches("(?:[0-9]{2})+"), "ITF requires an even number of digits");
                case "ean8" -> value = gtin(value, 7, caption);
                case "ean13" -> value = gtin(value, 12, caption);
                case "upca" -> value = gtin(value, 11, caption);
                case "code128" -> {
                    require(value.length() <= 512 && value.chars().allMatch(c -> c <= 127), "Code 128 requires at most 512 ASCII bytes");
                    // In a Zebra ^BC field '>' introduces subset and function codes.
                    // Use explicit subset A numeric pairs for literal '>' in these records.
                    if (!caption && value.indexOf('>') >= 0) {
                        commands = commands.replace(",N,N,N,A" + DATA, ",N,N,N,N" + DATA);
                        value = literalCode128(value);
                    }
                }
                case "pdf417" -> {
                    require(value.chars().allMatch(c -> c <= 127), "PDF417 requires ASCII");
                    Matcher params = PDF417.matcher(commands);
                    require(params.find(), "invalid PDF417 parameters");
                    int columns = params.group(2).isEmpty() ? 30 : Integer.parseInt(params.group(2));
                    int capacity = Math.min(928, columns * 90) - (2 << Integer.parseInt(params.group(1))) - 1;
                    long digits = value.chars().filter(c -> c >= '0' && c <= '9').count();
                    // Even optimal compaction cannot fit more than three digits or two text symbols per word.
                    require(2 * digits + 3 * (value.length() - digits) <= 6L * capacity,
                            "exceeds PDF417 capacity for the configured columns and correction");
                }
                case "datamatrix" -> {
                    int maxWords = commands.contains(",|,2" + DATA) ? 49 : 1558;
                    // One word holds at most two digits; C40/Text can pack three
                    // single-byte letters into two words. Mixed modes are valid.
                    int sixths = 0;
                    for (byte b : value.getBytes(StandardCharsets.UTF_8)) {
                        if (b < 0) sixths += 6;
                        else if (b >= '0' && b <= '9') sixths += 3;
                        else sixths += 4;
                    }
                    require(sixths <= maxWords * 6, "exceeds Data Matrix symbol capacity");
                    value = value.replace("|", "||"); // ^BX explicitly selects '|'.
                }
                case "qr" -> {
                    int level = "LMQH".indexOf(prefix.charAt(0));
                    // Minimum bits for numeric, alphanumeric and byte segments, in sixths of a bit.
                    int sixths = 0;
                    for (byte b : value.getBytes(StandardCharsets.UTF_8)) {
                        if (b >= '0' && b <= '9') sixths += 20;
                        else if ("ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:".indexOf(b) >= 0) sixths += 33;
                        else sixths += 48;
                    }
                    int dataWords = new int[]{2956, 2334, 1666, 1276}[level];
                    require(sixths <= dataWords * 48, "exceeds QR capacity at the selected error correction");
                }
                case "aztec" -> {
                    require(value.getBytes(StandardCharsets.UTF_8).length <= 3832, "exceeds Aztec symbol capacity");
                }
                default -> throw new IllegalArgumentException("Unknown field type " + type);
            }
            if (caption) value = blockText(value);
            String data = prefix + fieldHex(value);
            require(data.length() <= 3072, "exceeds the ZPL field-data limit");
            return commands + data + "^FS";
        } catch (IllegalArgumentException e) {
            throw new IllegalArgumentException(String.join(", ", names) + " (" + kind + "): " + e.getMessage(), e);
        }
    }

    private static String expand(String expression, Map<String, String> values, Set<String> names) {
        Matcher parts = PART.matcher(expression);
        StringBuilder text = new StringBuilder();
        int pos = 0;
        while (parts.find()) {
            require(parts.start() == pos, "Invalid template field at offset " + pos);
            String name = parts.group(1);
            if (name == null) {
                ByteArrayOutputStream bytes = new ByteArrayOutputStream();
                for (int i = pos + 1; i < parts.end(); i += 3) {
                    bytes.write(Integer.parseInt(expression, i, i + 2, 16));
                }
                text.append(bytes.toString(StandardCharsets.UTF_8));
            } else {
                names.add(name);
                String value = values.get(name);
                require(value != null, "Missing variable: " + name);
                text.append(value);
            }
            pos = parts.end();
        }
        require(pos == expression.length() && !names.isEmpty(), "Malformed field expression");
        return text.toString();
    }

    private static String gtin(String value, int digits, boolean caption) {
        require((value.length() == digits || value.length() == digits + 1) && value.matches("[0-9]+"),
                "expected " + digits + " digits with an optional check digit");
        int sum = 0;
        for (int i = 0; i < digits; i++) {
            sum += (value.charAt(digits - 1 - i) - '0') * (i % 2 == 0 ? 3 : 1);
        }
        int check = (10 - sum % 10) % 10;
        require(value.length() == digits || value.charAt(digits) == '0' + check, "incorrect check digit");
        String body = value.substring(0, digits);
        return caption ? body + check : body;
    }

    private static String literalCode128(String text) {
        StringBuilder out = new StringBuilder(">9");
        boolean setA = true;
        for (char c : text.toCharArray()) {
            boolean needsA = c < 96;
            if (needsA != setA) {
                out.append(needsA ? ">7" : ">6");
                setA = needsA;
            }
            if (setA) {
                int code = c < 32 ? c + 64 : c - 32;
                out.append((char) ('0' + code / 10)).append((char) ('0' + code % 10));
            } else {
                out.append(c);
            }
        }
        return out.toString();
    }

    private static String blockText(String text) {
        return text.replace("\\", "\\\\").replace("\r\n", "\n").replace('\r', '\n').replace("\n", "\\&");
    }

    private static String fieldHex(String value) {
        StringBuilder data = new StringBuilder();
        for (byte b : value.getBytes(StandardCharsets.UTF_8)) {
            int c = b & 255;
            if (c >= 33 && c <= 126 && "^~_{}".indexOf(c) < 0) {
                data.append((char) c);
            } else {
                data.append('_').append(HEX[c >>> 4]).append(HEX[c & 15]);
            }
        }
        return data.toString();
    }

    private static void require(boolean valid, String message) {
        if (!valid) throw new IllegalArgumentException(message);
    }
}
