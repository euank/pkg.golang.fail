package main

import (
	"context"
	"fmt"
	"net/http"

	// import a 3-ary tuple
	tuple3 "pkg.golang.fail/tuple/3/tuple"
)

func main() {
	ctx := context.Background()

	// normally, sending multiple things over a channel is awkward. Tuple to the rescue!
	firstResult := make(chan tuple3.Tuple[string, *http.Response, error])
	ctx, cancel := context.WithCancel(ctx)

	// see which of 3 sites responds quickest by request them all in parallel.
	for _, site := range []string{"https://google.com", "https://google.jp", "https://google.co.uk"} {
		go func(site string) {
			req, _ := http.NewRequestWithContext(ctx, "GET", site, nil)
			resp, err := http.DefaultClient.Do(req)
			select {
			case firstResult <- tuple3.New(site, resp, err):
			case <-ctx.Done():
			}
		}(site)
	}

	site, res, err := (<-firstResult).Unpack()
	cancel()

	fmt.Printf("The first result came from %q\n", site)
	if err != nil {
		fmt.Printf("It was an error response: %v\n", err)
	} else {
		fmt.Printf("There were no errors, status code was %v", res.StatusCode)
	}
}
