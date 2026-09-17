import React from 'react';
import { Route, Routes } from 'react-router-dom';
export function App() { return <Routes><Route path="/users" element={<Users />} /></Routes>; }
function Users() { return <div>Users</div>; }
