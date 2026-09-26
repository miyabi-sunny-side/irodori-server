export interface RouteMatch {
  index: number;
  params: Record<string, string>;
}

interface RouteDef {
  pattern: RegExp;
  params: string[];
  title: string;
}

// The index order matches the pages rendered by App.svelte.
export const routes: RouteDef[] = [
  { pattern: /^\/$/, params: [], title: "音声作成" },
  { pattern: /^\/dictionary$/, params: [], title: "読み辞書" },
  { pattern: /^\/history$/, params: [], title: "生成履歴" },
  { pattern: /^\/credits$/, params: [], title: "クレジット・利用条件" },
  { pattern: /^\/references$/, params: [], title: "お手本" },
];

export function matchRoute(pathname: string): RouteMatch {
  for (const [index, route] of routes.entries()) {
    const found = pathname.match(route.pattern);
    if (found) {
      const params: Record<string, string> = {};
      route.params.forEach((name, position) => {
        params[name] = decodeURIComponent(found[position + 1]);
      });
      return { index, params };
    }
  }
  return { index: 0, params: {} };
}
