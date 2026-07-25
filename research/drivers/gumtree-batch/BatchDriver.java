/*  This file is part of the CodeDiff code diffing tool.
 *
 *  Copyright (C) 2026 Marko Ivankovic
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published
 *  by the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

import com.github.gumtreediff.actions.Diff;
import com.github.gumtreediff.io.ActionsIoUtils;
import com.github.gumtreediff.matchers.GumtreeProperties;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;

import java.io.BufferedOutputStream;
import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.io.StringWriter;
import java.nio.charset.StandardCharsets;
import java.util.Map;

/**
 * A one-JVM-for-the-whole-run alternative to invoking `gumtree textdiff` once per fixture as a
 * fresh subprocess (see `../../../src/bin/benchmark_other.rs`'s `gumtree_line_labels`, which does
 * exactly that). That per-fixture subprocess model pays JVM startup (class loading, JIT warmup)
 * on every single invocation - dominating the measured time for small files, per
 * `benchmark_other_runtime.png`'s gumtree violin sitting almost flat around ~400ms regardless of
 * file size. This driver stays up for an entire batch, so only the first request in a run pays
 * startup cost; everything after it measures GumTree's own parse+match+edit-script work, JIT-
 * warmed the way a long-lived service would see it - not "cost of invoking GumTree the way the
 * CLI is normally used."
 *
 * Protocol: one JSON object per line on stdin, one JSON object per line on stdout, in request
 * order, flushed after each response so a caller reading incrementally never blocks on buffering.
 *
 * Request:  {"id": "<caller-chosen fixture id>", "generator": "<gumtree generator id, e.g.
 *            java-jdt>", "before": "<path to a source file>", "after": "<path to a source file>"}
 * Response: {"id": "<echoed>", "ms": <double, time inside Diff.compute + JSON serialization only,
 *            excludes reading the request line and writing the response>, "matches": [...],
 *            "actions": [...]}  - "matches"/"actions" are byte-for-byte the same schema
 *            `gumtree textdiff -f JSON` produces (both paths go through the same
 *            `ActionsIoUtils.toJson`), so any JSON consumer written against the CLI's output
 *            reads this unchanged.
 *           {"id": "<echoed>", "error": "<exception message>"} on failure for that one pair -
 *            does not stop the batch, so one bad fixture doesn't lose every result after it.
 */
public class BatchDriver {
    public static void main(String[] args) throws Exception {
        // The CLI's own entry point (`com.github.gumtreediff.client.Run`) populates the
        // TreeGenerators/Matchers registries in a static initializer
        // (ClassIndex.getSubclasses(...).install(...)) before any command runs - skip that class
        // entirely (as this driver does, going straight to the `Diff`/`ActionsIoUtils` library
        // API) and `Diff.compute` fails with "No generator ... found" for every id, even valid
        // ones, since the registries are just empty. Referencing the class forces its static
        // initializer to run without pulling in any of its CLI argument-parsing machinery.
        Class.forName("com.github.gumtreediff.client.Run");

        BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        PrintStream out = new PrintStream(new BufferedOutputStream(System.out, 1 << 16), false, "UTF-8");

        String line;
        while ((line = in.readLine()) != null) {
            if (line.isBlank()) {
                continue;
            }
            JsonObject request = JsonParser.parseString(line).getAsJsonObject();
            String id = request.get("id").getAsString();
            JsonObject response = new JsonObject();
            response.addProperty("id", id);
            try {
                response = diffOne(request);
            } catch (Exception e) {
                response.addProperty("error", String.valueOf(e));
            }
            out.println(response);
            out.flush();
        }
    }

    private static JsonObject diffOne(JsonObject request) throws Exception {
        String generator = request.get("generator").getAsString();
        String before = request.get("before").getAsString();
        String after = request.get("after").getAsString();

        long started = System.nanoTime();
        Diff diff = Diff.compute(before, after, generator, null, new GumtreeProperties());
        StringWriter serialized = new StringWriter();
        ActionsIoUtils.toJson(diff.src, diff.editScript, diff.mappings).writeTo(serialized);
        double ms = (System.nanoTime() - started) / 1_000_000.0;

        // Reuses `gumtree textdiff -f JSON`'s own serializer (see this class's doc comment) rather
        // than hand-rolling the matches/actions shape a second time, so the two paths can never
        // silently drift apart - merge its "matches"/"actions" keys into the response object next
        // to "id"/"ms" instead of nesting, so a consumer written against the CLI's top-level JSON
        // needs no structural change to read this driver's output.
        JsonElement parsed = JsonParser.parseString(serialized.toString());
        JsonObject response = new JsonObject();
        for (Map.Entry<String, JsonElement> entry : parsed.getAsJsonObject().entrySet()) {
            response.add(entry.getKey(), entry.getValue());
        }
        response.addProperty("id", request.get("id").getAsString());
        response.addProperty("ms", ms);
        return response;
    }
}
