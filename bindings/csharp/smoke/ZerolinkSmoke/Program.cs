using System.Text.Json;
using System.Threading;
using Zerolink;

var endpoint = args.Length > 0 ? args[0] : "local";
var topic = "audio/asr/text";
var expected = "csharp-smoke";
byte[]? receivedPayload = null;
MsgHeader? receivedHeader = null;
using var done = new ManualResetEventSlim(false);

using var client = new ZerolinkClient(endpoint);
client.Subscribe(topic, (_, header, payload) =>
{
    receivedHeader = header;
    receivedPayload = payload;
    done.Set();
});

client.PublishString(topic, expected, traceId: 9001);
if (!done.Wait(TimeSpan.FromSeconds(3)))
{
    throw new Exception("timeout waiting callback");
}

client.Unsubscribe(topic);

if (receivedPayload is null || receivedHeader is null)
{
    throw new Exception("callback payload/header missing");
}

var text = ZerolinkSchemas.DecodeString(receivedPayload);
if (text != expected)
{
    throw new Exception($"unexpected payload: {text}");
}
if (receivedHeader.Value.SchemaId != ZerolinkSchemas.Utf8StringV1)
{
    throw new Exception($"unexpected schema id: {receivedHeader.Value.SchemaId}");
}
Console.WriteLine($"received={text} trace_id={receivedHeader.Value.TraceId}");

if (endpoint.StartsWith("daemon://", StringComparison.Ordinal))
{
    using JsonDocument health = client.Health(endpoint);
    var root = health.RootElement;
    if (!root.TryGetProperty("status", out var status) || status.GetString() != "ok")
    {
        throw new Exception("daemon health status is not ok");
    }
    Console.WriteLine("daemon_health=ok");
}

Console.WriteLine("csharp_smoke=ok");
