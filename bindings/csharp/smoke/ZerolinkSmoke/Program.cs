using System.Text.Json;
using System.Threading;
using Zerolink;

var endpoint = args.Length > 0 ? args[0] : "local";
var mode = args.Length > 1 ? args[1] : "pubsub";
var topic = args.Length > 2 ? args[2] : "audio/asr/text";
var message = args.Length > 3 ? args[3] : "csharp-smoke";
var timeoutMs = args.Length > 4 ? int.Parse(args[4]) : 3000;

using var client = new ZerolinkClient(endpoint);

switch (mode)
{
    case "pubsub":
        RunPubSub(client, topic, message, timeoutMs);
        if (endpoint.StartsWith("daemon://", StringComparison.Ordinal))
        {
            RunHealthCheck(client, endpoint);
        }
        Console.WriteLine("csharp_smoke=ok");
        break;
    case "publish-string":
        client.PublishString(topic, message, traceId: 9001);
        Console.WriteLine($"published={message}");
        break;
    case "subscribe-once":
        RunSubscribeOnce(client, topic, message, timeoutMs);
        Console.WriteLine("csharp_subscribe_once=ok");
        break;
    default:
        throw new Exception($"unknown mode: {mode}");
}

static void RunPubSub(ZerolinkClient client, string topic, string expected, int timeoutMs)
{
    byte[]? receivedPayload = null;
    MsgHeader? receivedHeader = null;
    using var done = new ManualResetEventSlim(false);

    client.Subscribe(topic, (_, header, payload) =>
    {
        receivedHeader = header;
        receivedPayload = payload;
        done.Set();
    });
    client.PublishString(topic, expected, traceId: 9001);

    if (!done.Wait(TimeSpan.FromMilliseconds(timeoutMs)))
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
}

static void RunSubscribeOnce(ZerolinkClient client, string topic, string expected, int timeoutMs)
{
    byte[]? receivedPayload = null;
    MsgHeader? receivedHeader = null;
    using var done = new ManualResetEventSlim(false);

    client.Subscribe(topic, (_, header, payload) =>
    {
        receivedHeader = header;
        receivedPayload = payload;
        done.Set();
    });

    if (!done.Wait(TimeSpan.FromMilliseconds(timeoutMs)))
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
        throw new Exception($"unexpected payload: {text} expected={expected}");
    }
    if (receivedHeader.Value.SchemaId != ZerolinkSchemas.Utf8StringV1)
    {
        throw new Exception($"unexpected schema id: {receivedHeader.Value.SchemaId}");
    }
    Console.WriteLine($"received={text} trace_id={receivedHeader.Value.TraceId}");
}

static void RunHealthCheck(ZerolinkClient client, string endpoint)
{
    using JsonDocument health = client.Health(endpoint);
    var root = health.RootElement;
    if (!root.TryGetProperty("status", out var status) || status.GetString() != "ok")
    {
        throw new Exception("daemon health status is not ok");
    }
    Console.WriteLine("daemon_health=ok");
}
