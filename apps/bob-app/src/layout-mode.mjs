const MOBILE_USER_AGENT = /Android|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini/i;

export function detectTerminalKind(userAgent = "") {
  return MOBILE_USER_AGENT.test(userAgent) ? "mobile" : "desktop";
}

export function detectViewportShape(width, height) {
  return Number(height) >= Number(width) ? "portrait" : "landscape";
}

export function resolveLayoutMode(terminalKind, viewportShape, width = 1024) {
  if (viewportShape === "portrait" || Number(width) <= 768) return "mobile-portrait";
  return "desktop-wide";
}

export function createLayoutState(winOrOpts = {}, nav = {}) {
  const width =
    winOrOpts?.innerWidth ?? winOrOpts?.width ?? 1024;
  const height =
    winOrOpts?.innerHeight ?? winOrOpts?.height ?? 768;
  const userAgent =
    nav?.userAgent ?? winOrOpts?.userAgent ?? "";
  const terminalKind = detectTerminalKind(userAgent);
  const viewportShape = detectViewportShape(width, height);
  const layoutMode = resolveLayoutMode(terminalKind, viewportShape, width);
  const isCompact = layoutMode !== "desktop-wide";
  return {
    width,
    height,
    terminalKind,
    aspect: viewportShape,
    viewportShape,
    layoutMode,
    isCompact,
    compactNavigation: isCompact,
  };
}

export function getProviderLogo(idOrModel = "") {
  const name = String(idOrModel || "").toLowerCase();
  if (name.includes("deepseek")) return "/logos/deepseek.png";
  if (name.includes("minimax")) return "/logos/minimax.png";
  if (name.includes("qwen") || name.includes("dashscope") || name.includes("aliyun"))
    return "/logos/qwen.png";
  if (name.includes("glm") || name.includes("zhipu")) return "/logos/glm.svg";
  if (name.includes("kimi") || name.includes("moonshot")) return "/logos/kimi.png";
  if (name.includes("doubao") || name.includes("seed") || name.includes("volcengine"))
    return "/logos/doubao.png";
  if (name.includes("gpt") || name.includes("o3") || name.includes("o4") || name.includes("openai"))
    return "/logos/openai.png";
  if (
    name.includes("gemini") ||
    name.includes("google") ||
    name.includes("gemma") ||
    name.includes("vertex")
  )
    return "/logos/gemini.svg";
  if (name.includes("claude") || name.includes("anthropic")) return "/logos/claude.png";
  if (name.includes("modelscope")) return "/logos/modelscope.png";
  if (name.includes("openrouter")) return "/logos/openrouter.png";
  if (name.includes("tavily")) return "/logos/tavily.png";
  if (name.includes("tinyfish")) return "/logos/tinyfish.svg";
  if (name.includes("grok") || name.includes("xai")) return "/logos/grok.png";
  if (name.includes("mimo")) return "/logos/mimo.png";
  return "/logos/openai.png";
}
