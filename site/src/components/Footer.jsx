import BrandMark from './BrandMark.jsx'
import { scrollToTop } from '../lib/scroll.js'
import { GITHUB_URL, CHANGELOG_URL, ISSUES_URL, PROFILE_URL } from '../lib/links.js'

const COLS = [
  { h: '产品', links: [['功能', '#capabilities'], ['使用场景', '#labs'], ['使用流程', '#method'], ['下载', '#cta']] },
  { h: '资源', links: [['快捷键说明', GITHUB_URL], ['常见问题', ISSUES_URL], ['更新日志', CHANGELOG_URL], ['联系我们', PROFILE_URL]] },
  { h: '项目', links: [['关于', '#about'], ['源代码', GITHUB_URL], ['提交反馈', ISSUES_URL]] },
  { h: '说明', links: [['隐私与权限', GITHUB_URL], ['开源协议 · MIT', GITHUB_URL]] },
]

export default function Footer() {
  return (
    <footer>
      <div className="container">
        <div className="foot-grid">
          <div className="foot-brand">
            <a className="brand" href="#top" onClick={scrollToTop}><BrandMark />Untype</a>
            <p>极简语音转文字。开口说话，标点与分段自动整理，文字直接落进你正在用的任何输入框。由 <a className="inline-link" href={GITHUB_URL} target="_blank" rel="noopener noreferrer">Untype</a> 打造。</p>
          </div>
          {COLS.map((col) => (
            <div className="foot-col" key={col.h}>
              <h5>{col.h}</h5>
              <ul>
                {col.links.map(([label, href]) => {
                  const external = href.startsWith('http')
                  return (
                    <li key={label}>
                      <a href={href} {...(external ? { target: '_blank', rel: 'noopener noreferrer' } : {})}>{label}</a>
                    </li>
                  )
                })}
              </ul>
            </div>
          ))}
        </div>
        <div className="foot-bottom">
          <span><span className="pulse" />© MMXXVI Untype · 语音转文字</span>
          <span className="right">
            <span>macOS</span>
            <span>本地优先</span>
            <span>zh-CN</span>
          </span>
        </div>
      </div>
      <div className="foot-mega">
        <div className="container wide"><div className="word">Untype<em>.</em></div></div>
      </div>
    </footer>
  )
}
