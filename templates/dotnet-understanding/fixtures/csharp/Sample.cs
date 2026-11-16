namespace PublicSample;

public sealed class Sample(string name)
{
    public string Describe() => $"{{sample}} {name,12:upper}";
    public string Quote() => @"A ""quoted"" value";
    public string Nested() => $"{Format("}", new[] { 1, 2 })}";
    public string Markup() => """<p title="raw">{literal}</p>""";
}
