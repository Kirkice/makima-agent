#!/usr/bin/env python3
"""
支持 Brotli 压缩的 WebGL 本地服务器

Unity WebGL 构建使用 .br (Brotli) 压缩文件，
需要发送 Content-Encoding: br 响应头。

使用方式:
    python webgl_server.py [端口号]
    默认端口: 8080
    访问: http://localhost:8080
"""

import http.server
import os
import sys


class BrotliHTTPRequestHandler(http.server.SimpleHTTPRequestHandler):
    """自定义 HTTP 请求处理器，支持 Brotli 压缩文件"""
    
    # 文件扩展名到 MIME 类型的映射
    EXTENSION_MAP = {
        '.html': 'text/html',
        '.js': 'application/javascript',
        '.css': 'text/css',
        '.json': 'application/json',
        '.png': 'image/png',
        '.jpg': 'image/jpeg',
        '.ico': 'image/x-icon',
        '.data': 'application/octet-stream',
        '.wasm': 'application/wasm',
        '.br': None,  # Brotli 压缩文件需要特殊处理
    }
    
    def end_headers(self):
        """添加 CORS 和缓存控制头"""
        self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
        self.send_header('Cross-Origin-Embedder-Policy', 'require-corp')
        self.send_header('Cache-Control', 'no-cache, no-store, must-revalidate')
        super().end_headers()
    
    def do_GET(self):
        """处理 GET 请求，对 .br 文件添加正确的 Content-Encoding"""
        
        # 获取请求的文件路径
        path = self.translate_path(self.path)
        
        # 检查是否是 .br 文件
        if path.endswith('.br'):
            # 去掉 .br 后缀获取原始文件路径
            original_path = path[:-3]
            
            # 检查原始文件是否存在（用于确定 MIME 类型）
            if os.path.exists(original_path):
                # 获取原始文件的 MIME 类型
                _, ext = os.path.splitext(original_path)
                content_type = self.EXTENSION_MAP.get(ext, 'application/octet-stream')
            else:
                content_type = 'application/octet-stream'
            
            # 发送响应
            try:
                with open(path, 'rb') as f:
                    content = f.read()
                
                self.send_response(200)
                self.send_header('Content-Type', content_type)
                self.send_header('Content-Encoding', 'br')
                self.send_header('Content-Length', str(len(content)))
                self.end_headers()
                self.wfile.write(content)
                
                # 日志
                self.log_message('"%s" %s (Brotli: %s)', self.path, 200, content_type)
                
            except Exception as e:
                self.send_error(500, str(e))
        else:
            # 非 .br 文件使用默认处理
            super().do_GET()
    
    def guess_type(self, path):
        """猜测文件的 MIME 类型"""
        _, ext = os.path.splitext(path)
        
        # 先检查自定义映射
        if ext in self.EXTENSION_MAP:
            return self.EXTENSION_MAP[ext]
        
        # 使用默认映射
        return super().guess_type(path)


def main():
    """启动服务器"""
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
    
    # 切换到 character-webgl 目录
    webgl_dir = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'character-webgl')
    if not os.path.exists(webgl_dir):
        webgl_dir = os.path.join(os.getcwd(), 'character-webgl')
    
    if not os.path.exists(webgl_dir):
        print(f"错误: 找不到 character-webgl 目录")
        print(f"当前目录: {os.getcwd()}")
        sys.exit(1)
    
    os.chdir(webgl_dir)
    
    print("=" * 60)
    print("WebGL 本地服务器 (支持 Brotli 压缩)")
    print("=" * 60)
    print(f"监听地址: http://localhost:{port}")
    print(f"WebGL 目录: {webgl_dir}")
    print()
    print("在浏览器中打开上面的地址")
    print("按 Ctrl+C 停止服务器")
    print("-" * 60)
    print()
    
    # 启动服务器
    server = http.server.HTTPServer(('', port), BrotliHTTPRequestHandler)
    
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\n\n[服务器] 服务器已停止")
        server.shutdown()


if __name__ == '__main__':
    main()