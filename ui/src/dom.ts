type Child = Node | string | null | undefined | false;

/** Minimal element builder: `h("div.cell.live", { style: "--c:red" }, child...)`. */
export function h(tag: string, attrs: Record<string, string> | null = null, ...children: Child[]): HTMLElement {
  const [name, ...classes] = tag.split(".");
  const el = document.createElement(name || "div");
  if (classes.length) el.className = classes.join(" ");
  for (const [k, v] of Object.entries(attrs ?? {})) el.setAttribute(k, v);
  for (const c of children) if (c || c === "") el.append(c);
  return el;
}

/** Replays a CSS animation on `el` by toggling `cls`. */
export function pulse(el: Element, cls: string): void {
  el.classList.remove(cls);
  void (el as HTMLElement).offsetWidth;
  el.classList.add(cls);
}
