/* VI · 实测（深色板：口语 → 成文 前后对比） */
export default function Work() {
  return (
    <section className="tight">
      <div className="work">
        <div className="work-rule">
          <span className="roman">VI.</span>
          <span>实测 · 一段口语 → 一段文字</span>
          <span>006 / 008</span>
        </div>
        <div className="work-grid">
          <div className="work-copy">
            <span className="label" data-reveal="">实测<span className="ix" style={{ color: 'rgba(247,241,222,.5)' }}>VI</span></span>
            <h2 data-reveal="">看它<br />怎样<em>工作</em><span className="dot">.</span></h2>
            <a className="work-link" href="#cta" data-reveal="">下载后亲自试一遍</a>
          </div>
          <a className="work-card" href="#cta" data-reveal="scale">
            <div className="label-row"><span className="small-label">你说的</span><span className="index">RAW · 01</span></div>
            <p className="said raw">「呃……那个我们明天上午十点开个会吧就聊一下下个版本的事情然后让设计也来一下」</p>
            <div className="meta-row"><span>未整理口语</span><span className="year">逐字</span></div>
          </a>
          <a className="work-card alt" href="#cta" data-reveal="scale">
            <div className="label-row"><span className="small-label">Untype 写下的</span><span className="index">OUT · 02</span></div>
            <p className="said">我们明天上午十点开个会，聊一下下个版本的事情，然后请设计也来一下。</p>
            <div className="meta-row"><span>自动标点 · 分段</span><span className="year">成文</span></div>
          </a>
        </div>
      </div>
    </section>
  )
}
