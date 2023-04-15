mod common;

use common::send_request;

#[test]
fn it_asks_about_shrek() {
    send_request("dali who's the voice actor of the donkey in Shrek?").unwrap();

    send_request("What other animated characters did he voice?").unwrap();
}
