import { asset } from '../lib/asset.js'

/* II · 关于 */
export default function About() {
  return (
    <section className="about" id="about">
      <div className="container">
        <div className="sec-rule">
          <span className="roman">II.</span>
          <span className="meta-grp"><span>关于 Untype</span><span className="dot-mark">·</span><span>为什么不打字</span></span>
          <span>002 / 008</span>
        </div>
        <div className="about-grid">
          <div>
            <span className="label" data-reveal="">关于<span className="ix">II</span></span>
            <h2 className="display" data-reveal="">不打字，<br />也能<em>成文</em><span className="dot">.</span></h2>
            <p className="lead" data-reveal="">键盘让思考迁就十指的速度。Untype 把这层阻力拿掉 —— 你只管说，它替你听写、标点、分段，再把整理好的文字放回光标所在的地方。</p>
            <p className="lead" data-reveal="" style={{ maxWidth: '44ch', color: 'var(--ink-mute)' }}>没有云端往返的等待，没有复制粘贴的来回。一段话从脑子到屏幕，只隔着一次开口。</p>
            <div className="footer-row" data-reveal="">
              <span className="mark">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round"><path d="M5 13c2-5 4-5 6 0s4 5 5 1" /><line x1="18" y1="8" x2="18" y2="18" /></svg>
              </span>
              <span>语音 · 标点 · 分段 · 落位</span>
              <span className="stamp"><span>EST. MMXXVI</span><span>由 Untype 打造</span></span>
            </div>
          </div>
          <div className="about-art" data-reveal="right">
            <span className="about-side-note"><b />声波振幅自左向右递减，收敛为一行文字</span>
            <img src={asset('assets/about.jpg')} alt="石膏头像与拱门：语音化作文字" />
            <span className="about-caption"><b>PLATE Nº 02</b>石膏 · 拱 · 基线</span>
          </div>
        </div>
      </div>
    </section>
  )
}
