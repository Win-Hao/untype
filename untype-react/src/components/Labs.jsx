import { useState } from 'react'
import { asset } from '../lib/asset.js'

const ArrowMark = () => (
  <span className="arrow-mark"><svg viewBox="0 0 24 24"><path d="M7 17L17 7M9 7h8v8" /></svg></span>
)

const PILLS = [
  { cat: 'all', label: '全部', count: '05' },
  { cat: 'write', label: '写作', count: '02' },
  { cat: 'talk', label: '沟通', count: '02' },
  { cat: 'dev', label: '开发', count: '01' },
]

const LABS = [
  { cat: 'write', img: 'lab-a', badge: '写作', n: '01 / 长文', tag: 'WRITE', title: '长文写作', body: '初稿、随笔、博客 —— 把脑中成段的话直接说出来，回头再改。' },
  { cat: 'talk', img: 'lab-b', badge: '沟通', n: '02 / 会议', tag: 'NOTE', title: '会议纪要', body: '边听边口述要点，散会时纪要已成段，不再事后补记。' },
  { cat: 'talk', img: 'lab-c', badge: '沟通', n: '03 / 回复', tag: 'CHAT', title: '即时回复', body: '微信、邮件、Slack —— 一句话回复，开口比打字快得多。' },
  { cat: 'dev', img: 'lab-d', badge: '开发', n: '04 / 代码', tag: 'CODE', title: '注释与提交', body: '写注释、commit message、给 AI 的提示词 —— 离开键盘也能想清楚。' },
  { cat: 'write', img: 'lab-e', badge: '写作', n: '05 / 速记', tag: 'IDEA', title: '灵感速记', body: '走路、通勤、洗碗时冒出的念头，一句话留住，不怕转瞬即逝。' },
]

/* IV · 适用场景（可点击筛选） */
export default function Labs() {
  const [active, setActive] = useState('all')
  const shown = active === 'all' ? LABS.length : LABS.filter((l) => l.cat === active).length

  return (
    <section id="labs">
      <div className="container">
        <div className="sec-rule">
          <span className="roman">IV.</span>
          <span className="meta-grp"><span>适用场景</span><span className="dot-mark">·</span><span>一天里能用到它的时刻</span></span>
          <span>004 / 008</span>
        </div>
        <div className="labs-head">
          <h2 className="display" data-reveal="">说，比打字<em>更快</em><span className="dot">.</span></h2>
          <div className="pills" data-reveal="">
            {PILLS.map((p) => (
              <button
                key={p.cat}
                className={'pill' + (active === p.cat ? ' active' : '')}
                onClick={() => setActive(p.cat)}
                type="button"
              >
                {p.label} <span className="count">{p.count}</span>
              </button>
            ))}
          </div>
        </div>
        <div className="labs-grid">
          {LABS.map((l) => {
            const visible = active === 'all' || l.cat === active
            return (
              <div className={'lab' + (visible ? '' : ' hide')} data-reveal="" key={l.img}>
                <div className="lab-img"><span className="badge">{l.badge}</span><img src={asset(`assets/${l.img}.jpg`)} alt={`${l.title}场景`} /></div>
                <div className="num-row"><span>{l.n}</span><span>{l.tag}</span></div>
                <h4>{l.title}</h4>
                <p>{l.body}</p>
                <ArrowMark />
              </div>
            )
          })}
        </div>
        <div className="labs-foot">
          <span>无处不在 · 任意输入框直接成文</span>
          <div className="progress">
            {[0, 1, 2, 3, 4].map((i) => <span key={i} className={i < shown ? 'on' : ''} />)}
          </div>
        </div>
      </div>
    </section>
  )
}
