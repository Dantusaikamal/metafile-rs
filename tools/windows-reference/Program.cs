if (args.Length < 2)
{
    Console.Error.WriteLine("usage: MetafileReference <generate|generate-affine|render> <path> [output width height]");
    return 2;
}

switch (args[0])
{
    case "generate" when args.Length == 2:
        MetafileReference.Generate(Path.GetFullPath(args[1]));
        return 0;
    case "generate-affine" when args.Length == 2:
        MetafileReference.GenerateAffine(Path.GetFullPath(args[1]));
        return 0;
    case "render" when args.Length == 5
        && int.TryParse(args[3], out var width)
        && int.TryParse(args[4], out var height):
        MetafileReference.Render(
            Path.GetFullPath(args[1]),
            Path.GetFullPath(args[2]),
            width,
            height);
        return 0;
    default:
        Console.Error.WriteLine("invalid arguments");
        return 2;
}
