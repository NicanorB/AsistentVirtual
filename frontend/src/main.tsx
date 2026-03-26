import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import AppAuth from './AppAuth.tsx'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <AppAuth />
  </StrictMode>,
)
