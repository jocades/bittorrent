use bytes::{Buf, BufMut, Bytes, BytesMut};
use tokio::io::AsyncReadExt;
use tracing::debug;

#[allow(unused)]
#[derive(Debug)]
enum Frame {
    Other { data: Bytes },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let mut stream = BytesMut::new();
    let foo = b"foo";
    stream.put_u32((foo.len() + 1) as u32);
    stream.put_u8(7);
    stream.extend(foo);
    stream.put_u32(0); // KeepAlive
    let bar = b"barbaz";
    stream.put_u32((bar.len() + 1) as u32);
    stream.put_u8(7);
    stream.extend(bar);

    let mut stream = stream.as_ref();
    debug!(?stream);

    let mut buf = BytesMut::new();
    let mut frames = Vec::new();
    loop {
        let n = stream.read_buf(&mut buf).await?;
        debug!(buf_len = buf.len(), read = n);
        if n == 0 {
            // We must have read all the data before we receive EOF.
            if buf.is_empty() {
                break;
            } else {
                // If we havent it means the peer closed the connection while sending a frame.
                // panic!("connection reset by peer");
                // In this example just keep going.j
            }
        }

        let len = buf.get_u32() as usize;
        if len == 0 {
            // `KeepAlive` message.
            debug!("KeepAlive");
            continue;
        };
        let kind = buf.get_u8();
        let data = buf.split_to(len - 1).freeze();
        debug!(?len, ?kind, ?data);

        let frame = match kind {
            7 => Frame::Other { data },
            n => unimplemented!("{n}"),
        };

        debug!(?frame, buf_len = buf.len(), ?buf);

        frames.push(frame);
        if frames.len() == 2 {
            break;
        }
    }

    debug!(?frames);

    Ok(())
}
