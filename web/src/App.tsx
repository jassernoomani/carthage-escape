import { RouterProvider, createBrowserRouter } from 'react-router'
import Home from './pages/Home'
import HostHome from './pages/HostHome'
import HostLogin from './pages/HostLogin'
import HostSession from './pages/HostSession'
import Join from './pages/Join'
import NotFound from './pages/NotFound'
import PlayerRoom from './pages/PlayerRoom'

const router = createBrowserRouter([
  { path: '/', element: <Home /> },
  { path: '/join', element: <Join /> },
  { path: '/play', element: <PlayerRoom /> },
  { path: '/host/login', element: <HostLogin /> },
  { path: '/host', element: <HostHome /> },
  { path: '/host/sessions/:id', element: <HostSession /> },
  { path: '*', element: <NotFound /> },
])

export default function App() {
  return <RouterProvider router={router} />
}
