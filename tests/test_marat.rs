mod common;

use common::send_request;

#[test]
fn it_creates_git_project() {
    send_request(
        "Find out what is the home directory.
        Next list files in it marat and
        Next show me the current user name and UID.",
    )
    .unwrap();

    send_request(
        "Create a new file hello.txt in the home directory.
        Next write hello world to it.",
    )
    .unwrap();

    send_request(
        "Create a new directory. Name it as you like.
        In this directory, create a new git project.
        Install git if necessary.
        Move the hello.txt file to this directory.
        Create a new branch main and commit the file with a relevant message.
        ",
    )
    .unwrap();
}
