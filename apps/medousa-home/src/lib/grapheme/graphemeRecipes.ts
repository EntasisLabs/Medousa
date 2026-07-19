export interface GraphemeRecipe {
  id: string;
  title: string;
  subtitle: string;
  scriptName: string;
  intent: string;
  body: string;
  flowName?: string;
}

export const GRAPHEME_STARTER_RECIPES: GraphemeRecipe[] = [
  {
    id: "hello",
    title: "Say hello",
    subtitle: "Print a short message",
    scriptName: "Hello world",
    intent: "Quick intro script",
    body: `glyph Main {
  core.echo(message: "Hello from Medousa!")
}`,
    flowName: "Hello automation",
  },
  {
    id: "web-search",
    title: "Search the web",
    subtitle: "DuckDuckGo search, top results",
    scriptName: "Web search",
    intent: "Search the web for a topic",
    body: `glyph Main {
  web.duckduckgo(query: "What's new in AI this week?", max_results: 3)
}`,
    flowName: "Web search",
  },
  {
    id: "pipe-demo",
    title: "Chain steps together",
    subtitle: "Pass a value into the next step",
    scriptName: "Simple pipeline",
    intent: "Learn how pipes connect steps",
    body: `query Demo on Any {
  set { message: "You wrote a real Grapheme script" }
    |> core.echo(message: $current.message)
}`,
    flowName: "Pipeline demo",
  },
  {
    id: "shell-echo",
    title: "Run a sandboxed command",
    subtitle: "Run echo in the sandbox",
    scriptName: "Shell echo",
    intent: "Run echo via shell.run",
    body: `query ShellEcho {
  shell.run(
    command: "echo hello from medousa",
    network: false,
    timeout_ms: 5000
  ) { exit_code stdout stderr backend sandboxed }
}`,
    flowName: "Shell echo",
  },
];

export function recipeById(id: string): GraphemeRecipe | undefined {
  return GRAPHEME_STARTER_RECIPES.find((recipe) => recipe.id === id);
}

export function applyRecipeToEditor(recipe: GraphemeRecipe) {
  return {
    name: recipe.scriptName,
    intent: recipe.intent,
    body: recipe.body,
  };
}
