// Three tiny, deterministic WMF samples generated entirely in the browser.
// The standard WMF header and drawing records are real binary metafile data.
const word = n => [n & 255, (n >>> 8) & 255];
const dword = n => [...word(n), ...word(n >>> 16)];
const record = (fn, params = []) => [...dword(3 + params.length), ...word(fn), ...params.flatMap(word)];
const rgb = hex => { const n = Number.parseInt(hex.slice(1), 16); return ((n & 255) << 16) | (n & 0xff00) | ((n >>> 16) & 255); };
function sampleBytes(drawing) {
  const commands = [
    record(0x020b, [0, 0]), record(0x020c, [480, 720]),
    record(0x02fa, [0, 2, 0, ...[rgb('#283a31') & 65535, rgb('#283a31') >>> 16]]),
    record(0x012d, [0]),
  ];
  for (const shape of drawing) {
    const color = rgb(shape.color);
    commands.push(record(0x02fc, [0, color & 65535, color >>> 16, 0]));
    commands.push(record(0x012d, [1]));
    if (shape.kind === 'polygon') {
      const points = shape.points;
      commands.push(record(0x0324, [points.length, ...points.flat()]));
    } else {
      commands.push(record(shape.kind === 'ellipse' ? 0x0418 : 0x041b, [shape.y + shape.h, shape.x + shape.w, shape.y, shape.x]));
    }
    commands.push(record(0x01f0, [1])); // DELETEOBJECT brush; next brush reuses slot 1
  }
  commands.push(record(0, []));
  const body = commands.flat();
  const sizeWords = 9 + body.length / 2;
  const maxRecord = Math.max(...commands.map(c => c.length / 2));
  const standard = [...word(1), ...word(9), ...word(0x0300), ...dword(sizeWords), ...word(2), ...dword(maxRecord), ...word(0)];
  // Aldus placeable header: magic, handle, bounding box, units per inch, reserved, XOR checksum.
  const placeableWords = [0xcdd7, 0x9ac6, 0, 0, 0, 720, 480, 1440, 0, 0];
  const checksum = placeableWords.reduce((a,b) => a ^ b, 0);
  const placeable = [...placeableWords.flatMap(word), ...word(checksum)];
  return new Uint8Array([...placeable, ...standard, ...body]);
}
export const samples = [
  { id: 'composition', name: 'color-composition.wmf', title: 'Color composition', description: 'Simple shapes and overlapping color.', drawing: [
    {kind:'rect', x:60,y:70,w:600,h:330,color:'#e7eee1'},
    {kind:'ellipse',x:95,y:115,w:245,h:245,color:'#8fae94'},
    {kind:'ellipse',x:300,y:110,w:245,h:245,color:'#d7a77c'},
    {kind:'rect',x:450,y:230,w:150,h:110,color:'#4e7762'},
  ]},
  { id: 'geometry', name: 'geometry-study.wmf', title: 'Geometry study', description: 'Polygons, circles and precise edges.', drawing: [
    {kind:'rect',x:70,y:70,w:580,h:340,color:'#f1e9d9'},
    {kind:'polygon',points:[[180,340],[320,105],[460,340]],color:'#668c9b'},
    {kind:'ellipse',x:255,y:160,w:130,h:130,color:'#e6b565'},
  ]},
  { id: 'pattern', name: 'retro-pattern.wmf', title: 'Retro pattern', description: 'A repeating grid of vector marks.', drawing: Array.from({length:12},(_,i)=>({
    kind:i%3===0?'rect':'ellipse',x:110+(i%4)*125,y:70+Math.floor(i/4)*130,w:92,h:92,
    color:['#8ca78b','#d4a285','#718d98','#c9c0a0'][i%4],
  }))},
];
export const getSampleFile = id => {
  const sample = samples.find(s => s.id === id);
  if (!sample) throw new Error('Unknown sample');
  return new File([sampleBytes(sample.drawing)], sample.name, {type:'application/octet-stream'});
};
