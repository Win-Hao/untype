import { asset } from '../lib/asset.js'

const PARTNERS = [
  { label: '文档', small: 'Docs', glyph: (<svg viewBox="0 0 40 32" fill="none" stroke="currentColor" strokeWidth="1.6"><rect x="6" y="3" width="22" height="26" rx="2" /><line x1="11" y1="10" x2="23" y2="10" /><line x1="11" y1="15" x2="23" y2="15" /><line x1="11" y1="20" x2="19" y2="20" /></svg>) },
  { label: '邮件', small: 'Mail', glyph: (<svg viewBox="0 0 40 32" fill="none" stroke="currentColor" strokeWidth="1.6"><rect x="5" y="7" width="26" height="18" rx="2" /><path d="M5 9l13 9 13-9" /></svg>) },
  { label: '聊天', small: 'Chat', glyph: (<svg viewBox="0 0 40 32" fill="none" stroke="currentColor" strokeWidth="1.6"><path d="M7 6h22v15H16l-7 5v-5H7z" /></svg>) },
  { label: '笔记', small: 'Notes', glyph: (<svg viewBox="0 0 40 32" fill="none" stroke="currentColor" strokeWidth="1.6"><rect x="9" y="4" width="18" height="24" rx="2" /><line x1="13" y1="10" x2="23" y2="10" /><line x1="13" y1="15" x2="23" y2="15" /></svg>) },
  { label: '终端', small: 'Code', glyph: (<svg viewBox="0 0 40 32" fill="none" stroke="currentColor" strokeWidth="1.6"><rect x="6" y="5" width="26" height="20" rx="2" /><path d="M11 12l4 3-4 3M18 18h6" /></svg>) },
  { label: '浏览器', small: 'Web', glyph: (<svg viewBox="0 0 40 32" fill="none" stroke="currentColor" strokeWidth="1.6"><circle cx="19" cy="15" r="11" /><path d="M8 15h22M19 4c4 3 4 19 0 22M19 4c-4 3-4 19 0 22" /></svg>) },
]

/* VII · 设计理念 + 兼容场景 */
export default function Testimonial() {
  return (
    <section>
      <div className="container">
        <div className="sec-rule">
          <span className="roman">VII.</span>
          <span className="meta-grp"><span>设计理念</span><span className="dot-mark">·</span><span>它该出现在哪里</span></span>
          <span>007 / 008</span>
        </div>
        <div className="testimonial-grid">
          <div className="testimonial-copy">
            <h2 data-reveal="">最好的输入法，<br />是你<em>根本不用</em>打字<span className="dot" style={{ color: 'var(--coral)' }}>.</span></h2>
            <div className="author" data-reveal="">
              <span className="avatar">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round"><path d="M4 13c2-5 4-5 6 0s4 5 5 1" /><line x1="17" y1="8" x2="17" y2="18" /></svg>
              </span>
              <p>Untype 团队<span>产品设计理念</span></p>
            </div>
            <div className="divider" />
            <p className="partners-text" data-reveal="">它安静地待在后台，在你需要的任何输入框里随叫随到：</p>
            <div className="partners">
              {PARTNERS.map((p) => (
                <a className="partner" data-reveal="" key={p.small}>
                  <span className="glyph">{p.glyph}</span>
                  <span>{p.label}</span>
                  <small>{p.small}</small>
                </a>
              ))}
            </div>
            <a className="read-more" href="#cta" data-reveal="">在你的应用里试试</a>
          </div>
          <div className="testimonial-art" data-reveal="right">
            <img src={asset('assets/testimonial.jpg')} alt="设计理念拼贴：石刻引号" />
          </div>
        </div>
      </div>
    </section>
  )
}
