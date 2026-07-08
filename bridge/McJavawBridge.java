import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;

/**
 * 本地 TCP 中继，以 javaw.exe 进程发起对外连接。
 * 部分 MC 加速器会劫持 javaw 流量，mc-tunnel 经此后缀可走加速链路。
 */
public class McJavawBridge {
    private static final int BUF = 1024 * 1024;
    private static final int SO_BUF = 4 * 1024 * 1024;

    public static void main(String[] args) throws Exception {
        if (args.length < 4) {
            System.err.println("usage: McJavawBridge listenHost listenPort remoteHost remotePort");
            System.exit(1);
        }
        String listenHost = args[0];
        int listenPort = Integer.parseInt(args[1]);
        String remoteHost = args[2];
        int remotePort = Integer.parseInt(args[3]);

        try (ServerSocket server = new ServerSocket(listenPort, 256, InetAddress.getByName(listenHost))) {
            while (true) {
                Socket client = server.accept();
                tune(client);
                Thread t = new Thread(() -> relay(client, remoteHost, remotePort), "relay");
                t.setDaemon(true);
                t.start();
            }
        }
    }

    static void tune(Socket s) throws IOException {
        s.setTcpNoDelay(true);
        s.setReceiveBufferSize(SO_BUF);
        s.setSendBufferSize(SO_BUF);
    }

    static void relay(Socket client, String host, int port) {
        try (Socket remote = new Socket()) {
            tune(remote);
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
        Thread t =
                new Thread(
                        () -> {
                            byte[] buf = new byte[BUF];
                            try {
                                int n;
                                while ((n = in.read(buf)) != -1) {
                                    out.write(buf, 0, n);
                                    if (n < BUF) {
                                        out.flush();
                                    }
                                }
                                out.flush();
                            } catch (IOException ignored) {
                            }
                        },
                        "pipe");
        t.setDaemon(true);
        t.start();
        return t;
    }
}
