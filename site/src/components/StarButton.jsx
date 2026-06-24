import { useEffect, useState } from 'react'
import { GITHUB_URL } from '../lib/links.js'

// 实时拉取仓库 star 数。初始按当前真实值 0 显示，拉到后更新；
// 请求失败则保留兜底值，保证按钮上始终有数字。
const REPO_API = 'https://api.github.com/repos/Win-Hao/untype'

function formatStars(n) {
  if (n >= 1000) return (n / 1000).toFixed(1).replace(/\.0$/, '') + 'k'
  return String(n)
}

/* GitHub Star 按钮：跳转仓库主页，顺带显示实时 star 数 */
export default function StarButton() {
  const [stars, setStars] = useState(0)

  useEffect(() => {
    let alive = true
    fetch(REPO_API)
      .then((r) => (r.ok ? r.json() : null))
      .then((d) => {
        if (alive && d && typeof d.stargazers_count === 'number') setStars(d.stargazers_count)
      })
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [])

  return (
    <a
      className="btn btn-ghost btn-star"
      href={GITHUB_URL}
      target="_blank"
      rel="noopener noreferrer"
      aria-label="在 GitHub 上给 Untype 点 Star"
    >
      <svg className="gh-mark" viewBox="0 0 24 24" aria-hidden="true">
        <path fill="currentColor" d="M12 .5C5.37.5 0 5.78 0 12.29c0 5.2 3.44 9.6 8.21 11.16.6.11.82-.25.82-.56 0-.28-.01-1.02-.02-2-3.34.71-4.04-1.58-4.04-1.58-.55-1.37-1.34-1.74-1.34-1.74-1.09-.73.08-.72.08-.72 1.21.08 1.84 1.22 1.84 1.22 1.07 1.8 2.81 1.28 3.5.98.11-.76.42-1.28.76-1.57-2.67-.3-5.47-1.31-5.47-5.83 0-1.29.47-2.34 1.24-3.17-.13-.3-.54-1.52.12-3.16 0 0 1.01-.32 3.3 1.21a11.6 11.6 0 0 1 3-.4c1.02 0 2.05.13 3 .4 2.29-1.53 3.3-1.21 3.3-1.21.66 1.64.25 2.86.12 3.16.77.83 1.24 1.88 1.24 3.17 0 4.53-2.81 5.52-5.49 5.81.43.37.81 1.1.81 2.22 0 1.6-.01 2.9-.01 3.29 0 .31.21.68.83.56A12.04 12.04 0 0 0 24 12.29C24 5.78 18.63.5 12 .5z" />
      </svg>
      <span className="star-label">
        Star<span className="star-count">{formatStars(stars)}</span>
      </span>
    </a>
  )
}
