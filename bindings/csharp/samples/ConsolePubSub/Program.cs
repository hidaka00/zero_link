using System.Threading;
using Zerolink;

var topic = "audio/asr/text";
using var client = new ZerolinkClient("local");
using var done = new ManualResetEventSlim(false);

client.Subscribe(topic, (t, header, payload) =>
{
    var text = ZerolinkSchemas.DecodeString(payload);
    Console.WriteLine($"{t} trace_id={header.TraceId} schema_id={header.SchemaId} payload={text}");
    done.Set();
});

client.PublishString(topic, "hello-csharp", traceId: 7001);
if (!done.Wait(TimeSpan.FromSeconds(2)))
{
    throw new Exception("timeout waiting callback");
}

client.Unsubscribe(topic);
