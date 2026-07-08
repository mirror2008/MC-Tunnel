/**
 * 简单 HTTP 代理（CONNECT + 明文 HTTP）
 *
 * ⚠️ 这不是 MC-Tunnel 服务端，不能替代 mc-tunnel server，也不能走 MC 加速通道。
 * 仅适合：浏览器/工具直接配 HTTP 代理，域名走 Cloudflare HTTPS。
 *
 * 环境变量（Workers 控制台 → Settings → Variables）：
 *   PROXY_TOKEN  可选，设置后需在请求头带 X-Proxy-Token
 */

const TOKEN = typeof PROXY_TOKEN !== "undefined" ? PROXY_TOKEN : "";

function needAuth(req) {
  if (!TOKEN) return false;
  return req.headers.get("X-Proxy-Token") !== TOKEN;
}

function json(status, body) {
  return new Response(JSON.stringify(body, null, 2), {
    status,
    headers: { "Content-Type": "application/json; charset=utf-8" },
  });
}

async function handleConnect(request) {
  const url = new URL(request.url);
  const parts = url.host.split(":");
  const hostname = parts[0];
  const port = parts[1] ? parseInt(parts[1], 10) : 443;

  if (!hostname || Number.isNaN(port)) {
    return new Response("Bad CONNECT target", { status: 400 });
  }

  let upstream;
  try {
    upstream = connect({ hostname, port });
    await upstream.opened;
  } catch (e) {
    return new Response(`connect failed: ${e.message}`, { status: 502 });
  }

  const { readable, writable } = new TransformStream();

  if (request.body) {
    request.body.pipeTo(upstream.writable).catch(() => {});
  } else {
    upstream.writable.close().catch(() => {});
  }

  upstream.readable.pipeTo(writable).catch(() => {});

  return new Response(readable, { status: 200 });
}

async function handlePlainHttp(request) {
  const url = new URL(request.url);
  const target = url.searchParams.get("url");
  if (!target) {
    return json(400, {
      error: "缺少 url 参数",
      example: "GET /?url=https://example.com",
    });
  }

  let targetUrl;
  try {
    targetUrl = new URL(target);
  } catch {
    return json(400, { error: "无效 url" });
  }
  if (!["http:", "https:"].includes(targetUrl.protocol)) {
    return json(400, { error: "仅支持 http/https" });
  }

  const headers = new Headers(request.headers);
  headers.delete("host");
  headers.delete("cf-connecting-ip");

  const resp = await fetch(targetUrl.toString(), {
    method: request.method,
    headers,
    body: ["GET", "HEAD"].includes(request.method) ? undefined : request.body,
    redirect: "manual",
  });

  return new Response(resp.body, {
    status: resp.status,
    headers: resp.headers,
  });
}

export default {
  async fetch(request) {
    if (needAuth(request)) {
      return json(401, { error: "需要正确的 X-Proxy-Token" });
    }

    if (request.method === "GET" && new URL(request.url).pathname === "/") {
      return json(200, {
        name: "MC-Tunnel Cloudflare HTTP Proxy",
        note: "这不是 MC-Tunnel TCP 服务端。MC-Tunnel + 游戏加速器请用 VPS + 域名 A 记录。",
        usage: {
          connect: "HTTP 代理 CONNECT（需支持 CONNECT 的客户端）",
          plain: "GET /?url=https://example.com",
        },
      });
    }

    if (request.method === "CONNECT") {
      return handleConnect(request);
    }

    return handlePlainHttp(request);
  },
};
