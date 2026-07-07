import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;

/**
 * 本地 TCP 中继，以 javaw.exe 进程发起对外连接。
 * 雷神加速器的 MC 通道按进程劫持 javaw，mc-tunnel 经此后缀可走加速链路。
 *
 * 用法: javaw -jar bridge.jar <listenHost> <listenPort> <remoteHost> <remotePort>
 */
public class McLeigodBridge {
    public static void main(String[] args) throws Exception {
        if (args.length < 4) {
            System.err.println("usage: McLeigodBridge listenHost listenPort remoteHost remotePort");
            System.exit(1);
        }
        String listenHost = args[0];
        int listenPort = Integer.parseInt(args[1]);
        String remoteHost = args[2];
        int remotePort = Integer.parseInt(args[3]);

        try (ServerSocket server = new ServerSocket(listenPort, 128, InetAddress.getByName(listenHost))) {
            while (true) {
                Socket client = server.accept();
                client.setTcpNoDelay(true);
                Thread t = new Thread(() -> relay(client, remoteHost, remotePort), "relay");
                t.setDaemon(true);
                t.start();
            }
        }
    }

    static void relay(Socket client, String host, int port) {
        try (Socket remote = new Socket()) {
            remote.setTcpNoDelay(true);
            remote.setReceiveBufferSize(2 * 1024 * 1024);
            remote.setSendBufferSize(2 * 1024 * 1024);
            remote.connect(new InetSocketAddress(host, port), 20_000);
            Thread up = pipe(client.getInputStream(), remote.getOutputStream());
            Thread down = pipe(remote.getInputStream(), client.getOutputStream());
            up.join();
            down.join();
        } catch (Exception ignored) {
        } finally {
            try {
                client.close();
            } catch (IOException ignored) {
            }
        }
    }

    static Thread pipe(InputStream in, OutputStream out) {
        Thread t = new Thread(
                () -> {
                    byte[] buf = new byte[262144];
                    try {
                        int n;
                        while ((n = in.read(buf)) != -1) {
                            out.write(buf, 0, n);
                            out.flush();
                        }
                    } catch (IOException ignored) {
                    }
                },
                "pipe");
        t.setDaemon(true);
        t.start();
        return t;
    }
}
