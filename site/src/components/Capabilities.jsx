import { asset } from '../lib/asset.js'

const ArrowMark = () => (
  <span className="arrow-mark"><svg viewBox="0 0 24 24"><path d="M7 17L17 7M9 7h8v8" /></svg></span>
)

const CARDS = [
  { num: '01', tag: '唤起', title: '全局快捷键', body: '一个快捷键，在任何 app 里随时开始听写，无需切换窗口。' },
  { num: '02', tag: '转写', title: '实时成文', body: '边说边出字，所见即所得，说完即定稿，不必等待。' },
  { num: '03', tag: '整理', title: '标点与分段', body: '自动断句、补标点、分段落，口语顺成清楚的书面文字。' },
  { num: '04', tag: '落位', title: '直接插入', body: '文字落进当前光标处，省去复制粘贴的来回奔波。' },
]

/* III · 核心功能 */
export default function Capabilities() {
  return (
    <section id="capabilities">
      <div className="container">
        <div className="sec-rule">
          <span className="roman">III.</span>
          <span className="meta-grp"><span>核心功能</span><span className="dot-mark">·</span><span>四件小事，做到极简</span></span>
          <span>003 / 008</span>
        </div>
        <div className="capabilities-grid">
          <div className="capabilities-art" data-reveal="left">
            <div className="corner tl" /><div className="corner br" />
            <span className="ribbon">FIG. <b>03</b> · 功能</span>
            <img src={asset('assets/capabilities.jpg')} alt="拱门与立柱：聆听与书写" />
          </div>
          <div className="capabilities-copy">
            <span className="label" data-reveal="">核心功能<span className="ix">III</span></span>
            <h2 className="display" data-reveal="">少即是<em>多</em><span className="dot">.</span></h2>
            <div className="cards">
              {CARDS.map((c) => (
                <div className="card" data-reveal="" key={c.num}>
                  <div className="num">{c.num}<span className="tag">{c.tag}</span></div>
                  <h3>{c.title}</h3>
                  <p>{c.body}</p>
                  <ArrowMark />
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
