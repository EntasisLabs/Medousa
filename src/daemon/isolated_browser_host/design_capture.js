(() => {
  // Fixed, read-only capture code in the browser's isolated execution context.
  const omitPinned = false;
  const width = Math.round(innerWidth), height = Math.round(innerHeight);
  const nodes = [], raster_regions = [], warnings = new Set();
  let visited = 0, characters = 0;
  const colors = new Map(), paint = new OffscreenCanvas(1, 1).getContext("2d", { willReadFrequently: true });
  const color = value => {
    if (!value || value === 'transparent') return 'transparent';
    const match = value.match(/^rgba?\(([^)]+)\)$/);
    if (!match) {
      if (colors.has(value)) return colors.get(value);
      // Computed colors can be oklch(), lab() or color(display-p3 …).
      // Resolve these through the browser's own color engine without sampling page pixels.
      paint.clearRect(0, 0, 1, 1); paint.fillStyle = value; paint.fillRect(0, 0, 1, 1);
      const rgba = [...paint.getImageData(0, 0, 1, 1).data];
      const result = rgba[3] ? '#' + rgba.map(x => x.toString(16).padStart(2, '0')).join('') : 'transparent';
      colors.set(value, result); return result;
    }
    const c = match[1].split(/[, /]+/).map(Number);
    if (c.some(x => !Number.isFinite(x))) return 'transparent';
    const alpha = c.length > 3 ? Math.round(Math.max(0, Math.min(1, c[3])) * 255) : 255;
    if (!alpha) return 'transparent';
    return '#' + c.slice(0, 3).map(x => Math.max(0, Math.min(255, Math.round(x))).toString(16).padStart(2, '0')).join('') + (alpha < 255 ? alpha.toString(16).padStart(2, '0') : '');
  };
  const add = (name, rect, content, opacity = 1, appearance, textLayout) => {
    if (nodes.length >= 1200) { warnings.add('The page has more layers than this capture can import.'); return; }
    const node = { id: 'page_' + nodes.length, name: name.slice(0, 60), parent_id: 'page_frame', visible: true, locked: false,
      transform: { x: rect.x, y: rect.y, width: Math.max(1, rect.width), height: Math.max(1, rect.height), rotation: 0 }, opacity, content };
    if (appearance) node.appearance = appearance;
    if (textLayout) node.text_layout = textLayout;
    nodes.push(node);
  };
  const raster = (element, bounds, opacity) => {
    if (raster_regions.length >= 20) { warnings.add('Some complex graphics were omitted at the image limit.'); return; }
    const x = Math.max(0, bounds.x), y = Math.max(0, bounds.y);
    const region = { asset_id: 'region_' + raster_regions.length, x, y, width: Math.min(width, bounds.right) - x, height: Math.min(height, bounds.bottom) - y };
    if (region.width < 1 || region.height < 1) return;
    raster_regions.push(region);
    // Screenshot pixels already include inherited opacity and clipping.
    add(element.getAttribute('aria-label') || element.tagName.toLowerCase() + ' image', region, { kind: 'image', asset_id: region.asset_id, crop: null }, 1);
    if (opacity < 1) warnings.add('Complex graphics retain the captured page background.');
  };
  function paragraph(element, style, bounds, opacity, name) {
    if (!['p','h1','h2','h3','h4','h5','h6','button','a','label','figcaption','pre'].includes(element.tagName.toLowerCase())) return false;
    if (element.querySelector('img,svg,canvas,input,textarea,[contenteditable],p,h1,h2,h3,div')) return false;
    let text = '', spans = [];
    const pre = style.whiteSpace.startsWith('pre');
    function collect(node) {
      if (node.nodeType === Node.TEXT_NODE) {
        const owner = node.parentElement, paint = getComputedStyle(owner);
        if (paint.display === 'none' || paint.visibility === 'hidden') return;
        let value = pre ? node.textContent : node.textContent.replace(/\s+/g, ' ');
        if (!pre && (!text || text.endsWith(' '))) value = value.replace(/^ +/, '');
        if (paint.textTransform === 'uppercase') value = value.toUpperCase();
        if (paint.textTransform === 'lowercase') value = value.toLowerCase();
        const start = text.length; text += value;
        if (value) spans.push({ start, end: text.length, font_weight: Math.max(100, Math.min(900, parseInt(paint.fontWeight) || 400)), font_style: paint.fontStyle === 'italic' ? 'italic' : 'normal', color: color(paint.color), underline: paint.textDecorationLine.includes('underline') });
      } else if (node.nodeType === Node.ELEMENT_NODE) {
        if (node.matches('[data-sensitive],[data-private],[contenteditable],input,textarea,script,style')) return;
        if (node.tagName === 'BR') text += '\n';
        else for (const child of node.childNodes) collect(child);
      }
    }
    collect(element); text = text.trimEnd();
    if (!text.trim()) return true;
    if (characters + text.length > 16000) { warnings.add('Some text was omitted at the capture limit.'); return true; }
    characters += text.length;
    spans = spans.map(s => ({...s, end: Math.min(s.end, text.length)})).filter(s => s.start < s.end);
    const size = Math.max(1, Math.min(512, parseFloat(style.fontSize) || 16));
    const padding = size * .06;
    let family = style.fontFamily.split(',')[0].replace(/["']/g, '').trim();
    if (!/^[\p{L}\p{N} _-]{1,128}$/u.test(family)) family = 'sans-serif';
    add(name + ' text', { x: bounds.x - padding, y: bounds.y - padding, width: bounds.width + padding * 2, height: bounds.height + padding * 2 },
      { kind: 'text', text, color: color(style.color), font_family: family, font_size: size, font_weight: Math.max(100, Math.min(900, parseInt(style.fontWeight) || 400)), font_style: style.fontStyle === 'italic' ? 'italic' : 'normal' }, opacity, undefined,
      { width: Math.max(4, bounds.width + padding * 2), align: ['left','center','right'].includes(style.textAlign) ? style.textAlign : 'left', line_height: Math.min(4, Math.max(.8, (parseFloat(style.lineHeight) || size * 1.3) / size)), letter_spacing: Math.max(-100, Math.min(100, parseFloat(style.letterSpacing) || 0)), paragraph_spacing: 0, underline: style.textDecorationLine.includes('underline'), spans });
    return true;
  }
  function visit(element, inheritedOpacity = 1) {
    if (++visited > 8000 || nodes.length >= 1200) { warnings.add('The page was imported with a bounded layer count.'); return; }
    const tag = element.tagName?.toLowerCase();
    if (!tag || ['script','style','link','meta','template','noscript','head'].includes(tag)) return;
    const style = getComputedStyle(element), bounds = element.getBoundingClientRect();
    const opacity = inheritedOpacity * Number(style.opacity);
    if (omitPinned && (style.position === 'fixed' || style.position === 'sticky' && bounds.top <= (parseFloat(style.top) || 0) + 1)) return;
    if (style.display === 'none' || style.visibility === 'hidden' || opacity <= 0) return;
    const visible = bounds.right > 0 && bounds.bottom > 0 && bounds.x < width && bounds.y < height && bounds.width > 0 && bounds.height > 0;
    const name = element.getAttribute('aria-label') || element.id || tag;
    const sensitive = element.matches('input,textarea,[contenteditable="true"],[data-sensitive],[data-private]');
    if (visible && !sensitive && (['img','svg','canvas','video','iframe','picture'].includes(tag) || style.filter !== 'none' || style.transform !== 'none' && !/^matrix\(1, 0, 0, 1,/.test(style.transform))) { raster(element, bounds, opacity); return; }
    if (visible) {
      const fill = color(style.backgroundColor), stroke = color(style.borderTopColor), border = parseFloat(style.borderTopWidth) || 0;
      if (fill !== 'transparent' || border > 0) add(name, bounds, { kind: 'shape', shape: 'rectangle', fill, stroke, stroke_width: border }, opacity,
        { corner_radius: Math.max(0, parseFloat(style.borderTopLeftRadius) || 0), shadow: null });
      if (style.backgroundImage !== 'none' || style.boxShadow !== 'none') warnings.add('CSS gradients and shadows may need adjustment in the editable copy.');
    }
    if (sensitive) return; // Never copy typed form values or editable drafts.
    if (visible && paragraph(element, style, bounds, opacity, name)) return;
    const children = [...element.children].sort((a, b) => (Number(getComputedStyle(a).zIndex) || 0) - (Number(getComputedStyle(b).zIndex) || 0));
    for (const child of element.childNodes) {
      if (child.nodeType !== Node.TEXT_NODE || !child.textContent?.trim() || !visible) continue;
      const value = child.textContent;
      if (characters + value.length > 16000) { warnings.add('Some text was omitted at the capture limit.'); break; }
      characters += value.length;
      const range = document.createRange(), runs = [];
      for (let i = 0; i < value.length; i++) {
        range.setStart(child, i); range.setEnd(child, i + 1);
        const r = range.getBoundingClientRect();
        if (!r.width || !r.height) continue;
        let run = runs[runs.length - 1];
        if (!run || Math.abs(run.y - r.y) > 2) { run = { text: '', x: r.x, y: r.y, width: 0, height: r.height }; runs.push(run); }
        run.text += value[i]; run.width = Math.max(run.width, r.right - run.x);
      }
      const size = parseFloat(style.fontSize) || 16, leading = Math.min(4, Math.max(.8, (parseFloat(style.lineHeight) || size * 1.3) / size));
      let family = style.fontFamily.split(',')[0].replace(/["']/g, '').trim();
      if (!/^[\p{L}\p{N} _-]{1,128}$/u.test(family)) family = 'sans-serif';
      for (const run of runs) {
        if (run.y >= height || run.y + run.height <= 0 || !run.text.trim()) continue;
        const padding = size * .06;
        add(name + ' text', { x: run.x - padding, y: run.y - (size * leading - run.height) / 2 - padding, width: run.width + padding * 2, height: size * leading + padding * 2 },
          { kind: 'text', text: run.text, color: color(style.color), font_family: family, font_size: Math.max(1, Math.min(512, size)), font_weight: Math.max(100, Math.min(900, parseInt(style.fontWeight) || 400)), font_style: style.fontStyle === 'italic' ? 'italic' : 'normal' }, opacity, undefined, { width: null, align: 'left', line_height: leading });
      }
    }
    for (const child of children) visit(child, opacity);
  }
  visit(document.documentElement);
  const background = color(getComputedStyle(document.documentElement).backgroundColor);
  const frame = { id: 'page_frame', name: 'Editable page', parent_id: null, visible: true, locked: false, opacity: 1, transform: { x: 0, y: 0, width, height, rotation: 0 }, content: { kind: 'frame', name: 'Editable page', background: background === 'transparent' ? '#ffffff' : background } };
  return { url: location.href, viewport: { width, height, scroll_x: scrollX, scroll_y: scrollY }, scene: { schema: 'medousa-create', version: 3, background: 'transparent', nodes: [frame, ...nodes] }, raster_regions, warnings: [...warnings] };
})()
