import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Map;

/** java OrderLabel SO-00123 "Thermal labels" order.zpl */
public final class OrderLabel {
    public static void main(String[] args) throws Exception {
        if (args.length != 3) throw new IllegalArgumentException("Usage: OrderLabel ORDER_NO PRODUCT OUTPUT.zpl");
        try (InputStream in = OrderLabel.class.getResourceAsStream("/shipping.tpl.zpl")) {
            if (in == null) throw new IllegalArgumentException("Add shipping.tpl.zpl to the classpath");
            String template = new String(in.readAllBytes(), StandardCharsets.UTF_8);
            byte[] job = ZplTemplate.fill(template, Map.of("order_no", args[0], "product", args[1]));
            Files.write(Path.of(args[2]), job);
        }
    }
}
