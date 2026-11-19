import fs from "node:fs";
const L = { type: "line" }, SL = { type: "line", soft: true }, HL = [{ type: "line", hard: true }, { type: "break-parent" }];
const LL = [{ type: "line", hard: true, literal: true }, { type: "break-parent" }];
const g = (contents, extra = {}) => ({ type: "group", contents, break: false, ...extra });
const ind = c => ({ type: "indent", contents: c }); const al = (n, c) => ({ type: "align", contents: c, n });
const cur = { type: "cursor" }, trim = { type: "trim" }, bp = { type: "break-parent" };
const words = s => s.split(" ").flatMap((w, i) => i ? [L, w] : [w]);
const docs = [];
const add = (doc, o = {}) => docs.push([doc, { printWidth: 80, tabWidth: 2, useTabs: false, endOfLine: "lf", ...o }]);
const call = (name, args) => g([name, "(", ind([SL, ...args.flatMap((a, i) => i ? [",", L, a] : [a])]), { type: "if-break", breakContents: ",", flatContents: "" }, SL, ")"]);
add(["   ", HL, "Prettier", HL]);
add(["123", cur, "Prettier  \t", cur, "\t \t", HL]);
add([cur, cur, cur]);
add(["a", cur, "b"]);
add(["a", cur, "bc", cur, "d", HL, "e"]);
for (const w of [10, 20, 40, 80]) for (const tabs of [false, true]) for (const tw of [2, 4, 8]) {
  add([call("someFunction", ["argumentNumberOne", "argumentNumberTwo", call("inner", ["x", "yy", "zzz"])]), ";", HL], { printWidth: w, useTabs: tabs, tabWidth: tw });
  add({ type: "fill", parts: words("the quick brown fox jumps over the lazy dog and keeps running far away") }, { printWidth: w, useTabs: tabs, tabWidth: tw });
  add(ind([HL, al(4, [HL, "four", al(2, [HL, "six", ind([HL, "x"])])]), HL, al("> ", [HL, "quoted", HL, "more"])]), { printWidth: w, useTabs: tabs, tabWidth: tw });
  add(ind([HL, "a", al(-1, [HL, "dedented"]), al({ $num: "-Infinity" }, [HL, "root"]), al({ type: "root" }, ind([HL, "marked", al({ $num: "-Infinity" }, [HL, "toRoot"])]))]), { printWidth: w, useTabs: tabs, tabWidth: tw });
}
for (const eol of ["lf", "crlf", "cr", "auto"]) add(["a\nb", HL, "c", ind([HL, "d"]), LL, "e"], { endOfLine: eol });
add(g(["const x = ", g(["[", ind([SL, "1", ",", L, "2"]), SL, "]"], { id: "g1" }), { type: "if-break", breakContents: " // broken", flatContents: " // flat", groupId: "g1" }]));
add(g(["const x = ", g(["[", ind([SL, "1", ",", L, "2", bp]), SL, "]"], { id: "g1" }), { type: "if-break", breakContents: " // broken", flatContents: " // flat", groupId: "g1" }, { type: "indent-if-break", contents: [HL, "y"], groupId: "g1" }, { type: "indent-if-break", contents: [HL, "z"], groupId: "g1", negate: true }]));
add(g(["x", { type: "indent-if-break", contents: [L, "y"], groupId: "nope" }]));
add(["a", { type: "line-suffix", contents: " // comment" }, "b", HL, "c", { type: "line-suffix", contents: " // end" }]);
add(g(["a", { type: "line-suffix", contents: " // c" }, { type: "line-suffix-boundary" }, L, "b"]));
add(["x", { type: "line-suffix-boundary" }, "y"]);
add({ type: "label", label: "member-chain", contents: g(["a", SL, ".b()"]) });
const states = [["f(", "a", ")"], ["f(", ind([SL, "a"]), SL, ")"], ["f(", ind([HL, "averyveryverylongargument"]), HL, ")"]];
for (const w of [3, 6, 30]) { add(g(states[0], { expandedStates: states }), { printWidth: w }); add(g(states[0], { expandedStates: states, break: true }), { printWidth: w }); add(g(states[0], { expandedStates: [states[0], states[1]] }), { printWidth: w }); }
for (const s of ["😀 emoji", "👨‍👩‍👧 family", "日本語テキスト", "é́", "a\u0000b\u007f\u0085", "🇹🇷 flag", "1️⃣ keycap", "©️ ©", "한국어", "ｆｕｌｌ", "‍​", { $utf16: [120, 0xd800, 121] }]) {
  for (const w of [8, 12]) add(g(["start ", ind([SL, s, L, s]), SL, "end"]), { printWidth: w });
  add({ type: "fill", parts: [s, L, s, L, s, L, s] }, { printWidth: 10 });
}
add({ type: "fill", parts: [] }); add({ type: "fill", parts: ["only"] }, { printWidth: 2 }); add({ type: "fill", parts: ["a", L] }, { printWidth: 0 });
add({ type: "fill", parts: [g(["x", HL, "y"]), L, "z"] });
add(["trailing   ", trim, "x  \t", trim, HL]);
add(g([g(["a", HL, "b"]), L, "c"]));
add(g(["a", L, g(["b", L, "c"], { break: true })]));
add(g(["a", { type: "if-break", breakContents: g(["x", bp]), flatContents: "y" }, L, "b"]));
add(g(["fits", L, "rest"]), { printWidth: 7 });
add(["", [], [[["deep"]]], ""]);
add(g(["a", L, "b"]), { printWidth: { $num: "Infinity" } });
add(g(["a  ", trim, "b", L, "c"]), { printWidth: 5 });
add(g(["a  ", trim, "b", L, "c"]), { printWidth: 80 });
add({ type: "fill", parts: ["a", L] });
add(g(["x", { type: "indent-if-break", contents: [L, "y"], groupId: "g9", negate: true }], { id: "g9" }));
add(g(["x", { type: "indent-if-break", contents: [HL, "y"], groupId: "g9", negate: true }, { type: "indent-if-break", contents: [HL, "z"], groupId: "g9" }], { id: "g9" }), { printWidth: 1 });
add(ind(["a", al({ type: "root" }, ind([HL, "b", LL, "literal", al(2, [LL, "x"])]))]));
add(ind(["a", LL, "noRoot"]));
add(g({ type: "weird" }, { expandedStates: [["a"]] }));
add(g({ type: "weird" }, { expandedStates: [["a"]] }), { printWidth: 0 });
add({ type: "bogus" });
add([1]);
add(g({ type: "unknown" }));
fs.writeFileSync(process.argv[2], docs.map(([d, o]) => JSON.stringify({ fn: "printDocToString", args: [d, o] })).join("\n") + "\n");
console.log(docs.length);
