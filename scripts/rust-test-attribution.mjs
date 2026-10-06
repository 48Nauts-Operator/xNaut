// A narrow source check, not a Rust parser: inspect direct no-argument/void
// functions only inside cfg(test) inline modules. Mask comments and literals
// first so source examples, long docs and quoted braces cannot change scope.
function codeOnly(source) {
  let out = '';
  const blank = (text) => text.replace(/[^\n]/g, ' ');
  for (let i = 0; i < source.length;) {
    let end = i;
    if (source.startsWith('//', i)) {
      end = source.indexOf('\n', i);
      if (end < 0) end = source.length;
    } else if (source.startsWith('/*', i)) {
      let depth = 1;
      end = i + 2;
      while (end < source.length && depth) {
        if (source.startsWith('/*', end)) { depth++; end += 2; }
        else if (source.startsWith('*/', end)) { depth--; end += 2; }
        else end++;
      }
    } else {
      const raw = /^(?:br|cr|r)(#*)"/.exec(source.slice(i));
      if (raw) {
        const stop = source.indexOf('"' + raw[1], i + raw[0].length);
        end = stop < 0 ? source.length : stop + 1 + raw[1].length;
      } else if (source[i] === '"') {
        end = i + 1;
        while (end < source.length) {
          if (source[end] === '\\') end += 2;
          else if (source[end++] === '"') break;
        }
      } else if (source[i] === "'") {
        // A lifetime such as 'static is code, while a quoted brace is not.
        const char = /^'(?:\\(?:u\{[^}]+\}|x[\da-fA-F]{2}|.)|[^'\\\r\n])'/u.exec(source.slice(i));
        if (char) end = i + char[0].length;
      }
    }
    if (end > i) { out += blank(source.slice(i, end)); i = end; }
    else out += source[i++];
  }
  return out;
}

function closeBrace(code, start) {
  let depth = 1;
  for (let i = start + 1; i < code.length; i++) {
    if (code[i] === '{') depth++;
    else if (code[i] === '}' && --depth === 0) return i;
  }
  return code.length;
}

export function unattributedTests(source) {
  const code = codeOnly(source);
  const found = [];
  const visited = new Set();
  function inspect(start, end) {
    if (visited.has(start)) return;
    visited.add(start);
    let boundary = start + 1;
    for (let i = boundary; i < end; i++) {
      if (code[i] === ';') boundary = i + 1;
      if (code[i] !== '{') continue;
      const header = code.slice(boundary, i);
      const fn = /\b(?:async\s+)?fn\s+(\w+)\s*\(\s*\)\s*$/.exec(header);
      if (fn && !/#\s*\[\s*(?:test|tokio::test|rstest)\b/.test(header)) found.push(fn[1]);
      const close = closeBrace(code, i);
      // An inner module inherits cfg(test); methods/nested helper functions do not.
      if (/\bmod\s+\w+\s*$/.test(header)) inspect(i, close);
      i = close;
      boundary = close + 1;
    }
  }
  const modules = /#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\](?:\s*#\[[^\]]*\])*\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{/g;
  for (const match of code.matchAll(modules)) {
    const start = match.index + match[0].length - 1;
    inspect(start, closeBrace(code, start));
  }
  return found;
}
