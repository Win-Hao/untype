/* 「随处可用」跑马灯条：两行反向滚动，内容复制两份以实现无缝循环 */
const ROW1 = ['备忘录', '邮件', '文档', '聊天', '终端', '浏览器', '代码编辑器', '搜索框', '社交媒体']
const ROW2 = [
  ['听写', 'DICTATE'], ['标点', 'PUNCTUATE'], ['分段', 'SEGMENT'],
  ['插入', 'INSERT'], ['本地优先', 'LOCAL'], ['全局快捷键', 'HOTKEY'],
]

function PlainItems() {
  return [...ROW1, ...ROW1].map((it, i) => (
    <span className="wire-item" key={i}>
      <span className="wire-dot">·</span>
      <span className="wire-name">{it}</span>
    </span>
  ))
}
function RoleItems() {
  return [...ROW2, ...ROW2].map((it, i) => (
    <span className="wire-item" key={i}>
      <span className="wire-dot">·</span>
      <span className="wire-name">{it[0]}</span>
      <span className="wire-role">{it[1]}</span>
    </span>
  ))
}

export default function Wire() {
  return (
    <div className="wire">
      <div className="container">
        <div className="wire-inner">
          <div className="wire-left">
            <span className="wire-mark"><span className="wire-pulse" /></span>
            <span className="wire-title"><b>随处可用</b><span>任意输入框 · 任意应用</span></span>
          </div>
          <div className="wire-rows">
            <div className="wire-row"><div className="marquee-track"><PlainItems /></div></div>
            <div className="wire-row reverse"><div className="marquee-track"><RoleItems /></div></div>
          </div>
        </div>
      </div>
    </div>
  )
}
